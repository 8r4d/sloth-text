//! Merging two computers' binders: the three-way merge at the heart of
//! syncing a whole project through the helper (CROSS-PLATFORM.md, 4c).
//!
//! `base` is the project as both sides last agreed (kept in this computer's
//! sync state), `ours` is this computer's now, `theirs` the helper's now.
//! Items are matched by id, never by name or place:
//!
//! - **One field at a time.** Title, place (folder and position) and each
//!   inspector field merge separately: if only one side changed a field,
//!   that change is taken.
//! - **Both changed the same field:** this computer's is kept, and a dated
//!   line goes into the item's Notes saying what the other had, so it can
//!   be seen and nothing's lost silently.
//! - **Added on either side:** kept.
//! - **Gone on one side** (emptied from its Trash): gone, unless the other
//!   side changed it (or its text, or anything in it) meanwhile. Moved to
//!   the Trash on one side but changed on the other: it stays out.
//! - **Moves that would put a folder inside itself** (each side moved one
//!   into the other): the move that makes the loop is undone.
//! - **Order:** a folder's order is taken from whichever side reordered it;
//!   items new from the other side go after the item they follow there.
//! - **The project's own settings** (statuses, labels, goals, compile
//!   choices…) merge whole: the side that changed them wins, this
//!   computer's if both did.
//!
//! A document's file name isn't merged: each computer names its own files,
//! and the helper knows documents by id (see sync.rs).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::Value;

use crate::project::{Manifest, Meta, Node, Role};

/// What a merge couldn't settle by itself and settled anyway: each also
/// noted in the item's Notes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clash {
    pub id: String,
    pub what: String,
}

/// One item, flattened: where it is and what it says.
#[derive(Clone, Debug, PartialEq)]
struct Item {
    parent: Option<String>,
    title: String,
    role: Option<Role>,
    path: Option<String>,
    file: Option<String>,
    meta: Meta,
}

/// Every item by id, and each folder's children in order (top level: "").
struct Flat {
    items: HashMap<String, Item>,
    order: HashMap<String, Vec<String>>,
}

fn flatten(binder: &[Node]) -> Flat {
    fn walk(nodes: &[Node], parent: Option<&str>, flat: &mut Flat) {
        flat.order.insert(parent.unwrap_or("").to_owned(), nodes.iter().map(|n| n.id.clone()).collect());
        for n in nodes {
            let item = Item { parent: parent.map(str::to_owned), title: n.title.clone(), role: n.role, path: n.path.clone(), file: n.file.clone(), meta: n.meta.clone() };
            flat.items.insert(n.id.clone(), item);
            walk(&n.children, Some(&n.id), flat);
        }
    }
    let mut flat = Flat { items: HashMap::new(), order: HashMap::new() };
    walk(binder, None, &mut flat);
    flat
}

/// Three-way: the side that changed wins; `None` when both changed it
/// differently (the caller keeps ours and notes theirs).
fn pick<T: PartialEq + Clone>(base: Option<&T>, ours: &T, theirs: &T) -> Option<T> {
    if ours == theirs || base == Some(theirs) {
        Some(ours.clone())
    } else if base == Some(ours) {
        Some(theirs.clone())
    } else {
        None
    }
}

/// The merged project, and what was settled for it.
pub struct Merged {
    pub manifest: Manifest,
    pub clashes: Vec<Clash>,
}

/// Merge `ours` and `theirs` from `base`. `edited_here` and `edited_there`
/// are documents whose text changed on each side since `base` (which the
/// binder doesn't show), so they're kept rather than deleted. `there` names
/// the other computer, and `today` dates the notes.
pub fn merge(base: &Manifest, ours: &Manifest, theirs: &Manifest, edited_here: &BTreeSet<String>, edited_there: &BTreeSet<String>, there: &str, today: &str) -> Merged {
    let (b, o, t) = (flatten(&base.binder), flatten(&ours.binder), flatten(&theirs.binder));
    let mut clashes = Vec::new();
    let mut note = |id: &str, what: String, items: &mut BTreeMap<String, Item>| {
        if let Some(item) = items.get_mut(id) {
            let line = format!("[{today}, from syncing] {what}");
            item.meta.notes = if item.meta.notes.is_empty() { line.clone() } else { format!("{}\n\n{line}", item.meta.notes) };
        }
        clashes.push(Clash { id: id.to_owned(), what });
    };

    // Which side changed an item at all since base (any field, or its text).
    let changed = |side: &Flat, id: &str, edited: &BTreeSet<String>| {
        edited.contains(id) || match (b.items.get(id), side.items.get(id)) {
            (Some(was), Some(now)) => was != now,
            (None, Some(_)) => true,
            _ => false,
        }
    };

    // 1. Each item, field by field.
    let ids: BTreeSet<&String> = o.items.keys().chain(t.items.keys()).collect();
    let mut items: BTreeMap<String, Item> = BTreeMap::new();
    let mut later_notes: Vec<(String, String)> = Vec::new();
    for id in ids {
        let (was, mine, other) = (b.items.get(id), o.items.get(id), t.items.get(id));
        let item = match (mine, other) {
            (Some(m), Some(th)) => {
                let mut item = m.clone();
                match pick(was.map(|w| &w.title), &m.title, &th.title) {
                    Some(title) => item.title = title,
                    None => later_notes.push((id.clone(), format!("The title was changed on both computers: kept \u{201c}{}\u{201d}; {there} had \u{201c}{}\u{201d}.", m.title, th.title))),
                }
                let (meta, meta_notes) = merge_meta(was.map(|w| &w.meta), &m.meta, &th.meta, there);
                item.meta = meta;
                later_notes.extend(meta_notes.into_iter().map(|n| (id.clone(), n)));
                match pick(was.map(|w| &w.parent), &m.parent, &th.parent) {
                    Some(parent) => item.parent = parent,
                    None => later_notes.push((id.clone(), format!("It was moved on both computers: kept it where it is here; {there} had moved it elsewhere."))),
                }
                Some(item)
            }
            // Only here: new here, or gone there (kept if changed here since).
            (Some(m), None) => (was.is_none() || changed(&o, id, edited_here)).then(|| m.clone()),
            // Only there: new there, or gone here (kept if changed there since).
            (None, Some(th)) => (was.is_none() || changed(&t, id, edited_there)).then(|| Item { path: th.path.clone(), ..th.clone() }),
            (None, None) => None,
        };
        if let Some(item) = item {
            items.insert(id.clone(), item);
        }
    }
    // An item gone on one side but with something kept inside it stays.
    let kept: Vec<String> = items.keys().cloned().collect();
    for id in kept {
        let mut parent = items[&id].parent.clone();
        let mut seen = BTreeSet::new();
        // (`seen`: folders moved into each other on the two sides would
        // otherwise go round for ever; step 3 sorts those out.)
        while let Some(p) = parent.filter(|p| seen.insert(p.clone())) {
            if !items.contains_key(&p)
                && let Some(item) = o.items.get(&p).or(t.items.get(&p))
            {
                items.insert(p.clone(), item.clone());
            }
            parent = items.get(&p).and_then(|i| i.parent.clone());
        }
    }

    // 2. Trashed on one side, changed on the other: out of the Trash again,
    // back where the side that changed it has it.
    let trash = items.iter().find(|(_, i)| i.role == Some(Role::Trash)).map(|(id, _)| id.clone());
    let in_trash = |items: &BTreeMap<String, Item>, id: &str| {
        let mut at = items.get(id).and_then(|i| i.parent.clone());
        let mut steps = 0;
        while let Some(p) = at.filter(|_| { steps += 1; steps <= items.len() }) {
            if Some(&p) == trash.as_ref() {
                return true;
            }
            at = items.get(&p).and_then(|i| i.parent.clone());
        }
        false
    };
    let ids: Vec<String> = items.keys().cloned().collect();
    for id in &ids {
        let trashed_here = o.items.get(id).is_some_and(|_| in_trash_flat(&o, id, trash.as_deref()));
        let trashed_there = t.items.get(id).is_some_and(|_| in_trash_flat(&t, id, trash.as_deref()));
        let was_trashed = b.items.contains_key(id) && in_trash_flat(&b, id, trash.as_deref());
        if in_trash(&items, id) && !was_trashed {
            let rescue = if trashed_there && !trashed_here && changed(&o, id, edited_here) {
                o.items.get(id).and_then(|i| i.parent.clone())
            } else if trashed_here && !trashed_there && changed(&t, id, edited_there) {
                t.items.get(id).and_then(|i| i.parent.clone())
            } else {
                None
            };
            if let Some(parent) = rescue.filter(|p| items.contains_key(p)) {
                items.get_mut(id).expect("listed").parent = Some(parent);
                later_notes.push((id.clone(), "It was moved to the Trash on one computer and changed on the other, so it's been kept out of the Trash.".to_owned()));
            }
        }
    }

    // 3. No folder inside itself: a move that makes a loop is undone (back
    // to where it was before, or else to the top folder it came from).
    let ids: Vec<String> = items.keys().cloned().collect();
    for id in &ids {
        if loops(&items, id) {
            let back = b.items.get(id).and_then(|i| i.parent.clone()).filter(|p| items.contains_key(p));
            items.get_mut(id).expect("listed").parent = back;
            if loops(&items, id) || items[id].parent.is_none() && items[id].role.is_none() {
                let top = o.items.get(id).and_then(|_| top_of(&o, id)).filter(|p| items.contains_key(p));
                items.get_mut(id).expect("listed").parent = top;
            }
            later_notes.push((id.clone(), "Moving it on both computers would have put it inside itself, so one move was undone.".to_owned()));
        }
    }
    // An item whose folder is gone goes to the top folder it was in.
    let ids: Vec<String> = items.keys().cloned().collect();
    for id in &ids {
        let parent = items[id].parent.clone();
        if parent.as_ref().is_some_and(|p| !items.contains_key(p)) {
            let top = top_of(&o, id).or_else(|| top_of(&t, id)).filter(|p| items.contains_key(p));
            items.get_mut(id).expect("listed").parent = top;
        }
    }
    for (id, what) in later_notes {
        note(&id, what, &mut items);
    }

    // 4. Order within each folder.
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for (id, item) in &items {
        children.entry(item.parent.clone().unwrap_or_default()).or_default().push(id.clone());
    }
    let mut order: HashMap<String, Vec<String>> = HashMap::new();
    for (parent, members) in &children {
        let members: BTreeSet<&String> = members.iter().collect();
        let side = |f: &Flat| -> Vec<String> { f.order.get(parent).map(|v| v.iter().filter(|i| members.contains(i)).cloned().collect()).unwrap_or_default() };
        let (base_order, ours_order, theirs_order) = (side(&b), side(&o), side(&t));
        let common = |v: &[String]| -> Vec<String> { v.iter().filter(|i| base_order.contains(i)).cloned().collect() };
        // The backbone: theirs if only they reordered, else ours.
        let theirs_reordered = common(&ours_order) == common(&base_order) && common(&theirs_order) != common(&base_order);
        let (mut list, other) = if theirs_reordered { (theirs_order.clone(), ours_order.clone()) } else { (ours_order.clone(), theirs_order.clone()) };
        // Everything else, after what it follows on the side it came from.
        let mut rest: Vec<&String> = other.iter().filter(|i| !list.contains(i)).collect();
        rest.extend(members.iter().copied().filter(|i| !list.contains(i) && !other.contains(i)));
        for id in rest {
            let source = if other.contains(id) { &other } else { &list };
            let at = source.iter().position(|x| x == id).unwrap_or(0);
            let after = source[..at].iter().rev().find_map(|prev| list.iter().position(|x| x == prev));
            list.insert(after.map_or(0, |p| p + 1), id.clone());
        }
        order.insert(parent.clone(), list);
    }

    // 5. Build the tree again.
    fn build(parent: &str, items: &BTreeMap<String, Item>, order: &HashMap<String, Vec<String>>) -> Vec<Node> {
        order
            .get(parent)
            .map(|ids| {
                ids.iter()
                    .map(|id| {
                        let i = &items[id];
                        Node { id: id.clone(), title: i.title.clone(), role: i.role, path: i.path.clone(), file: i.file.clone(), children: build(id, items, order), meta: i.meta.clone() }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    let binder = build("", &items, &order);

    // 6. The project's own settings, whole.
    let whole = |field: fn(&Manifest) -> Value| pick(Some(&field(base)), &field(ours), &field(theirs)).unwrap_or_else(|| field(ours));
    let mut manifest = ours.clone();
    manifest.binder = binder;
    macro_rules! settle {
        ($($field:ident),*) => {$(
            manifest.$field = serde_json::from_value(whole(|m| serde_json::to_value(&m.$field).unwrap_or(Value::Null))).unwrap_or_else(|_| ours.$field.clone());
        )*};
    }
    settle!(title, statuses, labels, profiles, compile, goals, kind, notepad, new_at_top, no_characters, characteristics);
    Merged { manifest, clashes }
}

/// Merge the inspector's fields one at a time; notes on what both changed.
fn merge_meta(base: Option<&Meta>, ours: &Meta, theirs: &Meta, there: &str) -> (Meta, Vec<String>) {
    let as_map = |m: &Meta| match serde_json::to_value(m) {
        Ok(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    let (b, o, t) = (base.map(as_map), as_map(ours), as_map(theirs));
    let keys: BTreeSet<&String> = o.keys().chain(t.keys()).chain(b.iter().flat_map(|m| m.keys())).collect();
    let mut out = serde_json::Map::new();
    let mut notes = Vec::new();
    for key in keys {
        let get = |m: &serde_json::Map<String, Value>| m.get(key).cloned().unwrap_or(Value::Null);
        let (was, mine, other) = (b.as_ref().map(get), get(&o), get(&t));
        let value = match pick(was.as_ref(), &mine, &other) {
            Some(v) => v,
            None => {
                // Notes are added to on both sides: keep both.
                if key == "notes" {
                    let (m, th) = (mine.as_str().unwrap_or(""), other.as_str().unwrap_or(""));
                    Value::String(format!("{m}\n\n[From {there}]\n{th}"))
                } else {
                    notes.push(format!("Its {} was changed on both computers: kept this one's; {there} had {}.", field_name(key), describe(&other)));
                    mine
                }
            }
        };
        if !value.is_null() {
            out.insert(key.clone(), value);
        }
    }
    (serde_json::from_value(Value::Object(out)).unwrap_or_else(|_| ours.clone()), notes)
}

fn field_name(key: &str) -> &str {
    match key {
        "target_words" => "target",
        "exclude_from_compile" => "compile setting",
        "on_ipad" => "web setting",
        "url" => "web address",
        other => other,
    }
}

fn describe(value: &Value) -> String {
    match value {
        Value::Null => "none".to_owned(),
        Value::String(s) => format!("\u{201c}{s}\u{201d}"),
        Value::Array(a) => a.iter().map(describe).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    }
}

fn in_trash_flat(f: &Flat, id: &str, trash: Option<&str>) -> bool {
    let mut at = f.items.get(id).and_then(|i| i.parent.clone());
    while let Some(p) = at {
        if Some(p.as_str()) == trash {
            return true;
        }
        at = f.items.get(&p).and_then(|i| i.parent.clone());
    }
    false
}

fn loops(items: &BTreeMap<String, Item>, id: &str) -> bool {
    let mut at = items.get(id).and_then(|i| i.parent.clone());
    let mut steps = 0;
    while let Some(p) = at {
        if p == id || steps > items.len() {
            return true;
        }
        steps += 1;
        at = items.get(&p).and_then(|i| i.parent.clone());
    }
    false
}

/// The top-level folder an item is in, on one side.
fn top_of(f: &Flat, id: &str) -> Option<String> {
    let mut at = f.items.get(id)?.parent.clone()?;
    while let Some(p) = f.items.get(&at).and_then(|i| i.parent.clone()) {
        at = p;
    }
    Some(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Place, Project};

    const TODAY: &str = "2026-11-25";

    /// A project with a chapter of three scenes, to change on each side.
    fn base() -> (Project, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("sloth-writer-merge-{}-{}", std::process::id(), crate::project::random_id(6)));
        let mut p = Project::create(&dir, "Book").unwrap();
        let chapter = p.add_folder(&Place::Into("manuscript".into()), "Chapter").unwrap();
        for t in ["A", "B", "C"] {
            p.add_document(&Place::Into(chapter.clone()), t).unwrap();
        }
        (p, dir)
    }

    fn id_of(m: &Manifest, title: &str) -> String {
        fn find(nodes: &[Node], title: &str) -> Option<String> {
            nodes.iter().find_map(|n| if n.title == title { Some(n.id.clone()) } else { find(&n.children, title) })
        }
        find(&m.binder, title).unwrap_or_else(|| panic!("no {title}"))
    }

    fn titles_in(m: &Manifest, folder: &str) -> Vec<String> {
        fn find<'a>(nodes: &'a [Node], title: &str) -> Option<&'a Node> {
            nodes.iter().find_map(|n| if n.title == title { Some(n) } else { find(&n.children, title) })
        }
        find(&m.binder, folder).map(|n| n.children.iter().map(|c| c.title.clone()).collect()).unwrap_or_default()
    }

    fn node<'a>(m: &'a Manifest, id: &str) -> &'a Node {
        fn find<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
            nodes.iter().find_map(|n| if n.id == id { Some(n) } else { find(&n.children, id) })
        }
        find(&m.binder, id).unwrap()
    }

    fn run(base: &Manifest, ours: &Manifest, theirs: &Manifest) -> Merged {
        merge(base, ours, theirs, &BTreeSet::new(), &BTreeSet::new(), "Writing PC", TODAY)
    }

    /// Change a side through the real project functions.
    fn side(p: &Project, change: impl FnOnce(&mut Project)) -> Manifest {
        let mut p = p.clone_for_test();
        change(&mut p);
        p.manifest
    }

    #[test]
    fn changes_to_different_fields_both_come_through() {
        let (p, dir) = base();
        let a = id_of(&p.manifest, "A");
        let ours = side(&p, |p| p.node_mut_for_test(&a).title = "Arrival".into());
        let theirs = side(&p, |p| p.node_mut_for_test(&a).meta.synopsis = "Mara lands.".into());
        let m = run(&p.manifest, &ours, &theirs);
        assert_eq!(node(&m.manifest, &a).title, "Arrival");
        assert_eq!(node(&m.manifest, &a).meta.synopsis, "Mara lands.");
        assert!(m.clashes.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn both_changing_one_field_keeps_ours_and_notes_theirs() {
        let (p, dir) = base();
        let a = id_of(&p.manifest, "A");
        let ours = side(&p, |p| p.node_mut_for_test(&a).title = "Arrival".into());
        let theirs = side(&p, |p| p.node_mut_for_test(&a).title = "Landing".into());
        let m = run(&p.manifest, &ours, &theirs);
        let n = node(&m.manifest, &a);
        assert_eq!(n.title, "Arrival");
        assert!(n.meta.notes.contains("Writing PC had \u{201c}Landing\u{201d}"), "{}", n.meta.notes);
        assert!(n.meta.notes.starts_with("[2026-11-25, from syncing]"));
        assert_eq!(m.clashes.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn notes_written_on_both_are_both_kept() {
        let (p, dir) = base();
        let a = id_of(&p.manifest, "A");
        let ours = side(&p, |p| p.node_mut_for_test(&a).meta.notes = "Check the tide tables.".into());
        let theirs = side(&p, |p| p.node_mut_for_test(&a).meta.notes = "Mara's coat is blue.".into());
        let m = run(&p.manifest, &ours, &theirs);
        let notes = &node(&m.manifest, &a).meta.notes;
        assert!(notes.contains("Check the tide tables.") && notes.contains("Mara's coat is blue."), "{notes}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn additions_on_both_sides_are_kept_in_place() {
        let (p, dir) = base();
        let chapter = id_of(&p.manifest, "Chapter");
        let a = id_of(&p.manifest, "A");
        let b = id_of(&p.manifest, "B");
        let ours = side(&p, |p| {
            p.add_document(&Place::After(a.clone()), "A2").unwrap();
        });
        let theirs = side(&p, |p| {
            p.add_document(&Place::After(b.clone()), "B2").unwrap();
            p.add_document(&Place::Into(chapter.clone()), "D").unwrap();
        });
        let m = run(&p.manifest, &ours, &theirs);
        assert_eq!(titles_in(&m.manifest, "Chapter"), ["A", "A2", "B", "B2", "C", "D"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_reorder_on_one_side_wins() {
        let (p, dir) = base();
        let a = id_of(&p.manifest, "A");
        let c = id_of(&p.manifest, "C");
        let ours = side(&p, |p| {
            p.add_document(&Place::After(c.clone()), "D").unwrap();
        });
        let theirs = side(&p, |p| {
            p.move_node(&a, &Place::After(c.clone())).unwrap();
        });
        let m = run(&p.manifest, &ours, &theirs);
        assert_eq!(titles_in(&m.manifest, "Chapter"), ["B", "C", "D", "A"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn emptied_from_the_trash_there_is_gone_unless_changed_here() {
        let (p, dir) = base();
        let a = id_of(&p.manifest, "A");
        let b = id_of(&p.manifest, "B");
        let theirs = side(&p, |p| {
            p.remove_for_test(&a);
            p.remove_for_test(&b);
        });
        // B was edited here meanwhile (its text): it stays.
        let edited: BTreeSet<String> = [b.clone()].into();
        let m = merge(&p.manifest, &p.manifest, &theirs, &edited, &BTreeSet::new(), "Writing PC", TODAY);
        assert_eq!(titles_in(&m.manifest, "Chapter"), ["B", "C"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn trashed_there_but_changed_here_stays_out_of_the_trash() {
        let (p, dir) = base();
        let a = id_of(&p.manifest, "A");
        let ours = side(&p, |p| p.node_mut_for_test(&a).meta.synopsis = "Rewritten.".into());
        let theirs = side(&p, |p| {
            p.trash(&a).unwrap();
        });
        let m = run(&p.manifest, &ours, &theirs);
        assert_eq!(titles_in(&m.manifest, "Chapter"), ["A", "B", "C"]);
        assert!(node(&m.manifest, &a).meta.notes.contains("kept out of the Trash"));
        // Trashed there and not touched here: trashed.
        let m = run(&p.manifest, &p.manifest, &theirs);
        assert_eq!(titles_in(&m.manifest, "Chapter"), ["B", "C"]);
        assert_eq!(titles_in(&m.manifest, "Trash"), ["A"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn something_added_inside_a_folder_deleted_there_keeps_the_folder() {
        let (p, dir) = base();
        let chapter = id_of(&p.manifest, "Chapter");
        let ours = side(&p, |p| {
            p.add_document(&Place::Into(chapter.clone()), "New").unwrap();
        });
        let theirs = side(&p, |p| p.remove_for_test(&chapter));
        let m = run(&p.manifest, &ours, &theirs);
        // The folder stays for what was added here; what was in it and
        // untouched here goes, as it did there.
        assert_eq!(titles_in(&m.manifest, "Chapter"), ["New"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn two_folders_moved_into_each_other_dont_loop() {
        let (mut p, dir) = base();
        let one = p.add_folder(&Place::Into("manuscript".into()), "One").unwrap();
        let two = p.add_folder(&Place::Into("manuscript".into()), "Two").unwrap();
        let ours = side(&p, |p| {
            p.move_node(&one, &Place::Into(two.clone())).unwrap();
        });
        let theirs = side(&p, |p| {
            p.move_node(&two, &Place::Into(one.clone())).unwrap();
        });
        let m = run(&p.manifest, &ours, &theirs);
        // Every item's still in the tree exactly once, and nothing's inside itself.
        let flat = flatten(&m.manifest.binder);
        assert_eq!(flat.items.len(), flatten(&p.manifest.binder).items.len());
        assert!(!m.clashes.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn project_settings_merge_whole() {
        let (p, dir) = base();
        let ours = side(&p, |p| p.manifest.goals.daily = Some(500));
        let theirs = side(&p, |p| p.manifest.statuses.push("Polished".into()));
        let m = run(&p.manifest, &ours, &theirs);
        assert_eq!(m.manifest.goals.daily, Some(500));
        assert!(m.manifest.statuses.contains(&"Polished".to_owned()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn nothing_changed_is_nothing_changed() {
        let (p, dir) = base();
        let m = run(&p.manifest, &p.manifest, &p.manifest);
        assert_eq!(m.manifest, p.manifest);
        let _ = std::fs::remove_dir_all(dir);
    }
}
