# Sloth Writer helper

A small PHP service that keeps copies of the scenes you mark "Available on
iPad" in Sloth Writer, so you can write in Safari on an iPad. It runs on
ordinary shared hosting (PHP and MySQL/MariaDB), built the same way as feets.

The Mac and the iPad never talk to each other: each syncs with the helper.
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
4. Open the page. The first visit asks for the setup password and lets you
   choose a username and password.
5. Make a token on that page and paste it, with the page's address, into
   Sloth Writer's settings.

The database tables are created on the first request.

## What's here

| File | What it does |
|------|--------------|
| `public/api.php` | The whole service: accounts, tokens, projects, scenes with versions, history, the inbox |
| `public/index.html`, `app.js`, `style.css` | The page: sign in and tokens (the iPad scene editor is next) |
| `writer-config.example.php` | The settings to copy and fill in |
| `tests/run.sh` | Runs the tests against a throwaway MariaDB and PHP server |

## Tests

```sh
helper/tests/run.sh
```

Needs `php`, `mariadb` and `node` (on a Mac: `brew install php mariadb node`).
Nothing touches a real database: each run makes its own in a temporary folder
and removes it afterwards.
