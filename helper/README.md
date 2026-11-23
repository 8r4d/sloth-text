# Sloth Writer helper

A small PHP service that keeps copies of the scenes you mark "Available on the
web" in Sloth Writer, so you can write in a web browser. It runs on
ordinary shared hosting (PHP and MySQL/MariaDB), built the same way as feets.

The Mac and the web editor never talk to each other: each syncs with the helper.
Every save says which version it was edited from, so an edit made in one
place is never silently written over by the other, and every earlier
version is kept.

## Deploy to your web host

1. Copy `writer-config.example.php` to `writer-config.php` and fill it in: a
   long random setup password, and your database's details.
2. Upload `writer-config.php` **one folder above** your website's public
   folder if your host allows it (otherwise next to `api.php`; the web server
   runs `.php` files rather than showing them, so it's still safe).
3. Upload everything in `public/` into your website folder, or a subfolder
   such as `writer/`. Use HTTPS (most hosts give a free certificate).
4. Open the page. While there's no account yet, it shows **Create My
   Account**: enter the setup password from `writer-config.php`, then choose
   your username and password. (That's the only way to make an account, and
   it only works once: afterwards the page asks you to sign in.)
   If you forget your password, **Forgot your password?** on the sign-in
   page sets a new one with the same setup password. Change it any time
   under **Account**.
5. Under **Account**, make a token and paste it, with the page's address,
   into Sloth Writer's Settings (web helper).
6. Open the page in any browser and sign in. On a phone or tablet, add it to
   the home screen (in Safari: **Share → Add to Home Screen**) to have it
   like an app.

### Behind Cloudflare

The page is `index.php` so that every script and stylesheet is addressed by
its file's modification time (`app.js?v=1763841234`): each upload is a new
address, so Cloudflare (and browsers) never serve a stale copy, with nothing to
bump by hand. The service worker is registered the same way. PHP's output
and the API (`Cache-Control: no-store`) aren't cached. Don't turn on a
Cloudflare rule that caches everything (HTML or PHP) for this subdomain.

The database tables are created on the first request.

## What's here

| File | What it does |
|------|--------------|
| `public/api.php` | The whole service: accounts, tokens, projects, scenes with versions, history, the inbox |
| `public/index.php` | The page's HTML, with each script and stylesheet versioned for Cloudflare |
| `public/app.js`, `style.css` | The web editor page: projects, scenes, writing with autosave, clashes, offline drafts, new scenes, each project's Mobile Notes (the scene `notepad`), a paragraph editor (switched on per device under Account), account |
| `public/sw.js`, `manifest.json`, icons | Opening without a connection, and Add to Home Screen |
| `writer-config.example.php` | The settings to copy and fill in |
| `tests/run.sh` | Runs the API and browser tests, then Sloth Writer's sync, against a throwaway MariaDB and PHP server |

## Tests

```sh
helper/tests/run.sh
```

Needs `php`, `mariadb` and `node` (on a Mac: `brew install php mariadb node`).
Nothing touches a real database: each run makes its own in a temporary folder
and removes it afterwards.
