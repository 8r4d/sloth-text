# Writer: the office look

A plan for interface touches in Sloth Writer inspired by the TV show
*Severance*: its retro office terminals, its rewards and its calm corporate
voice. Two pieces are already built: the start screen's **card file**
(1.45.0) and the **Severance** and **Severance Light** themes (1.46.0 and
1.47.0). Sizes are as in ROADMAP.md: **S** is an hour or two, **M** about a
day, **L** several days.

Everything here is written in Writer's own words and drawn in its own
shapes. It takes the feel of the show, not its logo, company name, artwork
or lines, so nothing is copied and it all still makes sense to someone who
hasn't seen the show.

## Decisions (made 2026-11-25: the recommendations below)

1. **How the touches are switched on.**
   - **Recommendation:** the purely visual ones (the emblem and the screen
     glow) come with the two Severance themes. The useful ones (progress
     bins, facts, rewards and the opening transition) work in any theme,
     each with its own setting in App Settings. That way the bins don't
     depend on picking a theme, and other themes don't get a CRT glow.
2. **What a "bin" is.** **Recommendation:** each top-level folder in the
   Manuscript (usually a chapter, or a part). A document's own target, set
   in the inspector, is used when there is one. Otherwise each bin's share is
   the manuscript target divided evenly.
3. **The rewards.** Sloth-themed rather than the show's (no finger traps or
   waffles): a leaf, a hammock, a slow parade, and so on. Each is drawn in
   egui shapes or with one of the sloth's poses, so no new artwork is
   needed.

---

## 1. Progress bar and bins (M–L) ✅ (1.49.0; the main-window strip is still to do)

The terminal's best idea, and useful as well: see where the book stands at
a glance.

| Part | Size | Approach |
|---|---|---|
| Top bar | S | In Focus mode, a thin bar across the top: the document's title on the left, the manuscript's percentage towards its target on the right (no target: the word count). Drawn in the theme's accent, in a smaller font. |
| Bins | M | Along the bottom in Focus mode, a bin per top-level Manuscript folder: its number, a short title, and a bar filling towards its share. More than about 8: they scroll, the current chapter's always in view. Words come from the counts statistics already make (`stats_ui::chapters`). |
| The lid | S | When a bin passes its share, its lid lifts and settles once (about 0.6 s), and a small tick stays. Remembered in `.sloth/`, so it happens once per bin, not every session. Reduce motion: no movement, just the tick. |
| Outside Focus mode | S | An option to show the same strip under the editor in the main window. Off by default. |
| Settings | S | App Settings → Writing: "Show progress bins in Focus mode" (on by default). |

Test: a project with three chapters and a target; the bins show the right
shares, and passing one marks it done once.

## 2. Rewards that grow (M) ✅ (1.50.0)

The sloth already cheers when the day's goal is reached (`goal_ui.rs`). This
gives the cheer a ladder.

| Milestone | Reward |
|---|---|
| Day's goal (now) | The ring pulses, the sloth cheers. Unchanged. |
| 3 days in a row | A leaf for the sloth: a small card with the sloth and a line ("Three days running. Have a leaf.") |
| 7 days | A hammock: the asleep pose in a drawn hammock |
| 14 and 30 days | Bigger cards, each with its own line |
| Manuscript at 25%, 50%, 75% | A card naming the milestone and the date |
| Manuscript target reached | The party: a full-window moment with falling leaves (confetti), the cheering sloth and the book's title, then a quiet record of the day. Reduce motion: still, no falling. |

- Each reward is shown once, recorded in `.sloth/stats.ron` with its date.
- A **Rewards** shelf in Statistics lists what's been earned, with dates.
- Follows the existing "Celebrate reaching the day's goal" setting, renamed
  "Celebrate goals and streaks".

Test: a streak crossing 3 and 7 days earns each reward once; turning the
setting off shows none.

## 3. Facts about your work (S) ✅ (1.48.0)

A calm, true statement under the start screen's greeting, in a gentle
corporate voice, chosen fresh each day.

- Made from statistics Writer already keeps (`stats.rs`: the year's days,
  streaks, average, sprints, targets). Each fact is a small function that
  returns a sentence, or nothing when it doesn't apply.
- Examples: "You wrote 1,204 words on Tuesday." "Bellingham is 38% of the
  way to its target." "Your longest run of writing days is 12." "You write
  most on Sunday mornings." "Morning Pages hasn't been opened in three
  weeks. It is still there."
- Chosen by the date, so it stays the same all day and changes the next.
  Clicking it shows another.
- App Settings → Appearance: "Show a fact on the start screen" (on).

Test: each fact appears only when its numbers exist; the same date gives the
same fact.

## 4. The elevator (S–M) ✅ (1.50.0)

Opening a project feels like arriving: the start screen slides up and away,
the project rises in, about 0.4 s. The project's title shows briefly in the
middle, like a floor name.

- egui can't take a picture of the old screen, so a panel in the theme's
  window colour covers everything, carries the title, and slides up off the
  window as the project appears beneath it. Closing a project runs it in
  reverse.
- Reduce motion: no slide, the project just appears.
- App Settings → Appearance: "Slide between the start screen and a
  project" (on).

Test: with motion off, opening is immediate; with it on, the project is
usable as soon as the panel has gone, and keys pressed meanwhile aren't
lost.

## 5. Screen glow (M, the riskiest) ✅ (1.50.0; checked by eye, not by the contrast test)

The retro monitor's look, for the Severance themes only, and off by
default.

- egui can't make text itself glow without custom drawing, so this is an
  overlay over the editor: faint horizontal scanlines (one line in every
  three, at a few percent opacity) and a soft vignette darkening the
  corners. Both are cheap to draw.
- App Settings → Appearance, shown only with a Severance theme: "Old screen
  glow" (off).
- Checked against the readability rule the themes already pass: the
  overlay must not take the text below its contrast threshold.

Test: a screenshot with it on, checked by eye; the contrast check with the
overlay included.

## 6. The emblem (S) ✅ (1.48.0)

On the start screen with a Severance theme, the sloth becomes a
one-colour emblem: the reading pose tinted in the theme's accent, inside a
thin ring, like a company crest. Other themes keep the colour sloth.

Test: a screenshot in each Severance theme.

## The web editor (2.0.0)

With a Severance theme in Writer, the web page takes on its office look:
headings in the accent, a rule under the bar, a soft glow on the dark
theme's headings, and the old screen's scanlines and vignette when that's
on in Writer (sent with the theme's colours as `look.office` and
`look.glow`).

## Suggested order

1. Facts (3) and the emblem (6): an hour or two each, and they set the tone.
2. Progress bar and bins (1): the most useful, and the most of the show.
3. Rewards (2): built on the bins' and statistics' records.
4. The elevator (4).
5. Screen glow (5), last and optional.
