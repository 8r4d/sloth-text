<?php
// Sloth Writer helper configuration: an example with placeholders, safe to
// keep in git. Copy it to writer-config.php and fill in real values. That
// copy holds passwords, so it stays out of git (see .gitignore).
//
// On your host, put writer-config.php ONE FOLDER ABOVE the website folder if
// you can (so it can't be reached from the web at all). If not, put it next
// to api.php: still safe, because the web server runs .php files rather than
// showing them.

return [
    // Setup password: creates your account on the very first visit, and
    // later unlocks "Forgot password?". Long and random, used nowhere else.
    'app_password' => 'change-me-to-something-long-and-random',

    'mysql' => [
        'host'     => 'your-mysql-host.example.com',
        'port'     => 3306,
        'user'     => 'your-database-user',
        'password' => 'your-database-password',
        'database' => 'your-database-name',
    ],
];
