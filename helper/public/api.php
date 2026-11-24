<?php
// Sloth Writer's helper: keeps copies of the scenes marked "Available on the web"
// so they can be edited in a browser, and syncs them with Sloth Writer on the Mac.
// One PHP file over MariaDB, for ordinary shared hosting. The accounts code is
// feets' (see apps (web)/feets), trimmed to what one writer needs.
//
// Two kinds of caller:
//   the web editor  signs in with a password and gets a session cookie;
//   the Mac         sends "Authorization: Bearer <token>", a token made on the
//                   helper's settings page (only its hash is stored).
//
// Accounts
//   GET  api.php?action=me                 {user, needsSetup}
//   POST api.php?action=setup              {setupPassword, username, password}  the first account only
//   POST api.php?action=login              {username, password}
//   POST api.php?action=logout
//   POST api.php?action=change_password    {current, password}
//   POST api.php?action=recover            {setupPassword, username, password}  locked out
//
// Tokens for the Mac (signed in)
//   GET  api.php?action=tokens             [{id, label, createdAt, lastUsedAt}]
//   POST api.php?action=token_create       {label} → {token}: shown this once
//   POST api.php?action=token_delete&id=N
//
// Projects and scenes (signed in, or a token). p = the project's id from Writer.
//   GET  api.php?action=projects                       [{id, title, scenes, updatedAt}]
//   POST api.php?action=index_put&p=P                  {title, index, stats?}  the scenes marked for the web,
//        and the day's writing on the Mac: {date, today, goal, total, target, deadline}
//   GET  api.php?action=index&p=P                      {title, index, seq, stats, syncedAt, countedFrom}
//        countedFrom: the Mac's words today include what reached it before this
//        time (its sync before last), so the web editor adds only what it wrote after.
//   A scene's notes are the scene "<id>-notes", saved and synced like any other.
//   GET  api.php?action=scene&p=P&id=S                 {id, text, version}
//   POST api.php?action=scene_put&p=P&id=S             {text, base} → {version, seq, savedAt}
//        base: the version the text was edited from (0: a scene new here). If
//        the scene has moved on since, nothing is saved: 409 with {current}.
//   GET  api.php?action=changes&p=P&since=N            {seq, scenes: [{id, text, version, by, updatedAt}]}
//   GET  api.php?action=history&p=P&id=S               [{version, savedAt, by, words}] newest first
//   GET  api.php?action=history_text&p=P&id=S&version=V {text}
//   POST api.php?action=inbox_add&p=P                  {title, text, key?} → {id}  a new scene written here
//                                                      (sent again with the same key: the same one, not another)
//   GET  api.php?action=inbox&p=P                      [{id, title, text, createdAt}] not yet collected
//   POST api.php?action=inbox_collect&p=P              {ids}  the Mac has taken them into the binder

const SESSION_COOKIE = 'writer_session';
const SESSION_DAYS = 60;

// Password guessing: within LOGIN_WINDOW minutes, a username gets
// LOGIN_MAX_PER_USER tries and an IP address LOGIN_MAX_PER_IP. Past that,
// even the right password is refused until the window has passed.
const LOGIN_WINDOW = 15;
const LOGIN_MAX_PER_USER = 5;
const LOGIN_MAX_PER_IP = 20;

// Who saved a version: the Mac (with a token) or the web editor.
const BY_MAC = 'mac';
const BY_WEB = 'web';

const SCHEMA = [
    <<<SQL
    CREATE TABLE IF NOT EXISTS {users} (
      id            INT AUTO_INCREMENT PRIMARY KEY,
      username      VARCHAR(32)  NOT NULL UNIQUE,
      password_hash VARCHAR(255) NOT NULL,
      created_at    TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP
    ) CHARACTER SET utf8mb4
    SQL,
    <<<SQL
    CREATE TABLE IF NOT EXISTS {sessions} (
      token_hash  CHAR(64) PRIMARY KEY,  -- sha256 of the cookie value
      user_id     INT      NOT NULL,
      expires_at  DATETIME NOT NULL,
      INDEX (user_id)
    ) CHARACTER SET utf8mb4
    SQL,
    <<<SQL
    CREATE TABLE IF NOT EXISTS {login_attempts} (  -- recent password tries, see count_login_try()
      id        INT AUTO_INCREMENT PRIMARY KEY,
      username  VARCHAR(32) NOT NULL,  -- lower case; '' for the setup password
      ip        VARCHAR(45) NOT NULL,
      tried_at  TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
      INDEX (username, tried_at),
      INDEX (ip, tried_at)
    ) CHARACTER SET utf8mb4
    SQL,
    <<<SQL
    CREATE TABLE IF NOT EXISTS {api_tokens} (  -- the Mac's way in; only the hash is kept
      id           INT AUTO_INCREMENT PRIMARY KEY,
      user_id      INT          NOT NULL,
      token_hash   CHAR(64)     NOT NULL UNIQUE,
      label        VARCHAR(100) NOT NULL,
      created_at   TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
      last_used_at TIMESTAMP    NULL,
      INDEX (user_id)
    ) CHARACTER SET utf8mb4
    SQL,
    <<<SQL
    CREATE TABLE IF NOT EXISTS {projects} (
      id          INT AUTO_INCREMENT PRIMARY KEY,
      user_id     INT          NOT NULL,
      uid         VARCHAR(64)  NOT NULL,  -- the project's id, from Writer
      title       VARCHAR(200) NOT NULL,
      index_json  MEDIUMTEXT   NOT NULL,  -- the scenes marked for the web, as Writer describes them
      seq         BIGINT       NOT NULL DEFAULT 0,  -- counts every scene save, for changes since N
      updated_at  TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
      UNIQUE (user_id, uid)
    ) CHARACTER SET utf8mb4
    SQL,
    <<<SQL
    CREATE TABLE IF NOT EXISTS {scenes} (
      project_id  INT          NOT NULL,
      scene_id    VARCHAR(64)  NOT NULL,
      text        MEDIUMTEXT   NOT NULL,
      version     INT          NOT NULL,
      seq         BIGINT       NOT NULL,  -- the project's seq when last saved
      saved_by    VARCHAR(8)   NOT NULL,
      updated_at  TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
      PRIMARY KEY (project_id, scene_id),
      INDEX (project_id, seq)
    ) CHARACTER SET utf8mb4
    SQL,
    <<<SQL
    CREATE TABLE IF NOT EXISTS {scene_versions} (  -- every earlier version, never changed
      id          INT AUTO_INCREMENT PRIMARY KEY,
      project_id  INT          NOT NULL,
      scene_id    VARCHAR(64)  NOT NULL,
      version     INT          NOT NULL,
      text        MEDIUMTEXT   NOT NULL,
      saved_by    VARCHAR(8)   NOT NULL,
      saved_at    TIMESTAMP    NOT NULL,
      INDEX (project_id, scene_id, version)
    ) CHARACTER SET utf8mb4
    SQL,
    <<<SQL
    CREATE TABLE IF NOT EXISTS {inbox} (  -- new scenes written on the web, until the Mac collects them
      id          INT AUTO_INCREMENT PRIMARY KEY,
      project_id  INT          NOT NULL,
      title       VARCHAR(200) NOT NULL,
      text        MEDIUMTEXT   NOT NULL,
      created_at  TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP,
      collected   BOOLEAN      NOT NULL DEFAULT FALSE,
      client_key  VARCHAR(64)  NULL,  -- the page's own name for it, so sending it twice makes one
      INDEX (project_id, collected),
      UNIQUE KEY inbox_client_key (project_id, client_key)
    ) CHARACTER SET utf8mb4
    SQL,
];

// Columns added since a table was first made: added to tables made before.
// (Checked rather than ALTER … IF NOT EXISTS, which MySQL lacks.)
const UPGRADES = [
    ['inbox', 'client_key', 'ALTER TABLE {inbox} ADD COLUMN client_key VARCHAR(64) NULL,
                             ADD UNIQUE KEY inbox_client_key (project_id, client_key)'],
];

function send_json($data, int $status = 200): void {
    http_response_code($status);
    header('Content-Type: application/json');
    header('Cache-Control: no-store');
    echo json_encode($data);
    exit;
}

function fail(string $message, int $status, array $extra = []): void {
    send_json(['error' => $message] + $extra, $status);
}

set_exception_handler(function (Throwable $err) {
    error_log('Sloth Writer helper error: ' . $err->getMessage());
    send_json(['error' => $err->getMessage()], 500);
});

// The config file: one folder above the web root first (not reachable from
// the web at all), then next to this file.
$config = null;
foreach ([dirname(__DIR__) . '/writer-config.php', __DIR__ . '/writer-config.php'] as $path) {
    if (is_file($path)) {
        $config = require $path;
        break;
    }
}
if (!$config) {
    fail('writer-config.php not found', 500);
}

// Table names are written {users}, {scenes}… and get the configured prefix
// (writer_ unless the config says otherwise), so the helper can share a
// database with other sites (feets, say) without touching their tables.
final class Db extends PDO {
    public static string $prefix = 'writer_';

    public static function tables(string $sql): string {
        return preg_replace_callback('/\{(\w+)\}/', fn($m) => self::$prefix . $m[1], $sql);
    }

    public function prepare(string $query, array $options = []): PDOStatement|false {
        return parent::prepare(self::tables($query), $options);
    }

    public function query(string $query, ?int $fetchMode = null, mixed ...$fetchModeArgs): PDOStatement|false {
        return $fetchMode === null ? parent::query(self::tables($query)) : parent::query(self::tables($query), $fetchMode, ...$fetchModeArgs);
    }

    public function exec(string $statement): int|false {
        return parent::exec(self::tables($statement));
    }
}

function db(): PDO {
    global $config;
    static $pdo = null;
    if (!$pdo) {
        $c = $config['mysql'];
        $prefix = (string) ($config['table_prefix'] ?? 'writer_');
        if (!preg_match('/^[A-Za-z0-9_]*$/', $prefix)) {
            fail("writer-config.php: 'table_prefix' may only have letters, digits and _", 500);
        }
        Db::$prefix = $prefix;
        $pdo = new Db(
            "mysql:host={$c['host']};port={$c['port']};dbname={$c['database']};charset=utf8mb4",
            $c['user'],
            $c['password'],
            [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION, PDO::ATTR_DEFAULT_FETCH_MODE => PDO::FETCH_ASSOC]
        );
        foreach (SCHEMA as $sql) {
            $pdo->exec($sql);
        }
        foreach (UPGRADES as [$table, $column, $sql]) {
            if (!$pdo->query("SHOW COLUMNS FROM {{$table}} LIKE " . $pdo->quote($column))->fetch()) {
                $pdo->exec($sql);
            }
        }
    }
    return $pdo;
}

function read_json(): array {
    $data = json_decode(file_get_contents('php://input'), true);
    if (!is_array($data)) {
        throw new RuntimeException('Request body is not valid JSON');
    }
    return $data;
}

// --- Accounts (from feets) ---

// Hosts behind a proxy or load balancer report HTTPS in different ways.
function is_https(): bool {
    return (!empty($_SERVER['HTTPS']) && $_SERVER['HTTPS'] !== 'off')
        || ($_SERVER['REQUEST_SCHEME'] ?? '') === 'https'
        || ($_SERVER['SERVER_PORT'] ?? '') === '443'
        || strtolower($_SERVER['HTTP_X_FORWARDED_PROTO'] ?? '') === 'https'
        || strtolower($_SERVER['HTTP_X_FORWARDED_SSL'] ?? '') === 'on';
}

function set_session_cookie(string $value, int $expires): void {
    setcookie(SESSION_COOKIE, $value, [
        'expires' => $expires,
        'path' => '/',
        'secure' => is_https(),
        'httponly' => true,
        'samesite' => 'Strict',
    ]);
}

function start_session(int $userId): void {
    $token = bin2hex(random_bytes(32));
    db()->prepare('INSERT INTO {sessions} (token_hash, user_id, expires_at) VALUES (?, ?, NOW() + INTERVAL ' . SESSION_DAYS . ' DAY)')
        ->execute([hash('sha256', $token), $userId]);
    db()->exec('DELETE FROM {sessions} WHERE expires_at < NOW()');
    set_session_cookie($token, time() + SESSION_DAYS * 86400);
}

/// The bearer token sent by the Mac, if any.
function bearer_token(): string {
    $header = $_SERVER['HTTP_AUTHORIZATION'] ?? $_SERVER['REDIRECT_HTTP_AUTHORIZATION'] ?? '';
    if ($header === '' && function_exists('getallheaders')) {
        foreach (getallheaders() as $name => $value) {
            if (strcasecmp($name, 'Authorization') === 0) {
                $header = $value;
            }
        }
    }
    return preg_match('/^Bearer\s+(\S+)$/i', $header, $m) ? $m[1] : '';
}

// Who's calling: ['id', 'username', 'by'] (by: mac for a token, web for a
// session), or null.
function current_user(): ?array {
    $token = bearer_token();
    if ($token !== '') {
        $stmt = db()->prepare('SELECT u.id, u.username, t.id AS token_id FROM {api_tokens} t JOIN {users} u ON u.id = t.user_id WHERE t.token_hash = ?');
        $stmt->execute([hash('sha256', $token)]);
        $user = $stmt->fetch();
        if (!$user) {
            return null;
        }
        db()->prepare('UPDATE {api_tokens} SET last_used_at = NOW() WHERE id = ?')->execute([$user['token_id']]);
        return ['id' => (int) $user['id'], 'username' => $user['username'], 'by' => BY_MAC];
    }
    $cookie = $_COOKIE[SESSION_COOKIE] ?? '';
    if ($cookie === '') {
        return null;
    }
    $stmt = db()->prepare('SELECT u.id, u.username FROM {sessions} s JOIN {users} u ON u.id = s.user_id WHERE s.token_hash = ? AND s.expires_at > NOW()');
    $stmt->execute([hash('sha256', $cookie)]);
    $user = $stmt->fetch();
    return $user ? ['id' => (int) $user['id'], 'username' => $user['username'], 'by' => BY_WEB] : null;
}

function require_user(): array {
    $user = current_user();
    if (!$user) {
        fail('Not signed in', 401);
    }
    return $user;
}

// Behind Cloudflare, REMOTE_ADDR is Cloudflare's; the visitor's is in its header.
function client_ip(): string {
    return substr((string) ($_SERVER['HTTP_CF_CONNECTING_IP'] ?? $_SERVER['REMOTE_ADDR'] ?? ''), 0, 45);
}

// Counts a password try against the username and this IP, and refuses it if
// either has had too many lately. The try is saved before the password is
// checked, so requests sent all at once still see each other. '' for the
// setup password, which is only limited per IP. Returns the try's id.
function count_login_try(string $username): int {
    $pdo = db();
    $pdo->exec('DELETE FROM {login_attempts} WHERE tried_at < NOW() - INTERVAL 1 DAY');
    $name = mb_strtolower(mb_substr(trim($username), 0, 32));
    $ip = client_ip();
    $pdo->prepare('INSERT INTO {login_attempts} (username, ip) VALUES (?, ?)')->execute([$name, $ip]);
    $id = (int) $pdo->lastInsertId();
    $stmt = $pdo->prepare(
        'SELECT COALESCE(SUM(username = ? AND username <> \'\'), 0) AS by_name, COALESCE(SUM(ip = ?), 0) AS by_ip
         FROM {login_attempts} WHERE (username = ? OR ip = ?) AND tried_at > NOW() - INTERVAL ' . LOGIN_WINDOW . ' MINUTE'
    );
    $stmt->execute([$name, $ip, $name, $ip]);
    $tries = $stmt->fetch();
    if ($tries['by_name'] > LOGIN_MAX_PER_USER || $tries['by_ip'] > LOGIN_MAX_PER_IP) {
        fail('Too many tries. Wait ' . LOGIN_WINDOW . ' minutes and try again.', 429);
    }
    return $id;
}

// The password was right: forget this try, and the username's earlier ones.
function login_passed(int $tryId, string $username = ''): void {
    $name = mb_strtolower(trim($username));
    db()->prepare("DELETE FROM {login_attempts} WHERE id = ? OR (username = ? AND username <> '')")->execute([$tryId, $name]);
}

function new_password(string $password): string {
    if (strlen($password) < 8) {
        fail('Password must be at least 8 characters', 400);
    }
    return $password;
}

function needs_setup(): bool {
    return (int) db()->query('SELECT COUNT(*) FROM {users}')->fetchColumn() === 0;
}

function user_json(array $user): array {
    return ['username' => $user['username']];
}

function me(): array {
    $user = current_user();
    return ['user' => $user ? user_json($user) : null, 'needsSetup' => !$user && needs_setup()];
}

// Creates the first account. Guarded by app_password from writer-config.php,
// and only works while there are no accounts at all.
function setup(array $b): array {
    global $config;
    if (!needs_setup()) {
        fail('Setup has already been done', 403);
    }
    $try = count_login_try('');
    if (!hash_equals(trim($config['app_password']), trim($b['setupPassword'] ?? ''))) {
        fail('Wrong setup password', 403);
    }
    login_passed($try);
    $username = trim($b['username'] ?? '');
    if (!preg_match('/^[A-Za-z0-9_.-]{3,32}$/', $username)) {
        fail('Username must be 3-32 letters, numbers, dots, dashes or underscores', 400);
    }
    $password = new_password($b['password'] ?? '');
    db()->prepare('INSERT INTO {users} (username, password_hash) VALUES (?, ?)')
        ->execute([$username, password_hash($password, PASSWORD_DEFAULT)]);
    $id = (int) db()->lastInsertId();
    start_session($id);
    return user_json(['username' => $username]);
}

function find_user(string $username): ?array {
    $stmt = db()->prepare('SELECT id, username, password_hash FROM {users} WHERE username = ?');
    $stmt->execute([trim($username)]);
    return $stmt->fetch() ?: null;
}

function login(array $b): array {
    $try = count_login_try((string) ($b['username'] ?? ''));
    $user = find_user((string) ($b['username'] ?? ''));
    if (!$user || !password_verify($b['password'] ?? '', $user['password_hash'])) {
        usleep(random_int(400000, 800000));  // slow down password guessing
        fail('Wrong username or password', 403);
    }
    login_passed($try, $user['username']);
    start_session((int) $user['id']);
    return user_json($user);
}

function logout(): array {
    $token = $_COOKIE[SESSION_COOKIE] ?? '';
    if ($token !== '') {
        db()->prepare('DELETE FROM {sessions} WHERE token_hash = ?')->execute([hash('sha256', $token)]);
    }
    set_session_cookie('', 1);
    return ['ok' => true];
}

function set_password(int $userId, string $password): void {
    db()->prepare('UPDATE {users} SET password_hash = ? WHERE id = ?')
        ->execute([password_hash($password, PASSWORD_DEFAULT), $userId]);
    db()->prepare('DELETE FROM {sessions} WHERE user_id = ?')->execute([$userId]);
    start_session($userId);
}

function change_password(array $user, array $b): array {
    $try = count_login_try($user['username']);
    $full = find_user($user['username']);
    if (!password_verify($b['current'] ?? '', $full['password_hash'])) {
        usleep(random_int(400000, 800000));
        fail('Your current password is wrong', 403);
    }
    login_passed($try, $user['username']);
    set_password($user['id'], new_password($b['password'] ?? ''));
    return user_json($user);
}

// "Forgot password": app_password from writer-config.php unlocks the account.
function recover(array $b): array {
    global $config;
    $try = count_login_try('');
    $user = find_user($b['username'] ?? '');
    if (!$user || !hash_equals(trim($config['app_password']), trim($b['setupPassword'] ?? ''))) {
        usleep(random_int(400000, 800000));
        fail('Wrong setup password or username', 403);
    }
    login_passed($try, $user['username']);
    set_password((int) $user['id'], new_password($b['password'] ?? ''));
    return user_json($user);
}

// --- Tokens for the Mac ---

function list_tokens(int $userId): array {
    $stmt = db()->prepare('SELECT id, label, UNIX_TIMESTAMP(created_at) AS created, UNIX_TIMESTAMP(last_used_at) AS used
                           FROM {api_tokens} WHERE user_id = ? ORDER BY id');
    $stmt->execute([$userId]);
    return array_map(fn($t) => [
        'id' => (int) $t['id'], 'label' => $t['label'],
        'createdAt' => (int) $t['created'] * 1000,
        'lastUsedAt' => $t['used'] === null ? null : (int) $t['used'] * 1000,
    ], $stmt->fetchAll());
}

function create_token(int $userId, array $b): array {
    $label = trim((string) ($b['label'] ?? '')) ?: 'Sloth Writer';
    $token = bin2hex(random_bytes(32));
    db()->prepare('INSERT INTO {api_tokens} (user_id, token_hash, label) VALUES (?, ?, ?)')
        ->execute([$userId, hash('sha256', $token), mb_substr($label, 0, 100)]);
    return ['id' => (int) db()->lastInsertId(), 'token' => $token, 'label' => $label];
}

function delete_token(int $userId, int $id): array {
    db()->prepare('DELETE FROM {api_tokens} WHERE id = ? AND user_id = ?')->execute([$id, $userId]);
    return ['ok' => true];
}

// --- Projects and scenes ---

// The project's row, by Writer's id for it. With $create, a new one is made.
function project_row(int $userId, string $uid, bool $create = false, string $title = ''): ?array {
    if (!preg_match('/^[A-Za-z0-9_-]{1,64}$/', $uid)) {
        fail('That project id is not valid', 400);
    }
    $stmt = db()->prepare('SELECT id, uid, title, index_json, seq FROM {projects} WHERE user_id = ? AND uid = ?');
    $stmt->execute([$userId, $uid]);
    $row = $stmt->fetch();
    if (!$row && $create) {
        db()->prepare("INSERT INTO {projects} (user_id, uid, title, index_json) VALUES (?, ?, ?, '[]')")
            ->execute([$userId, $uid, $title]);
        return project_row($userId, $uid);
    }
    return $row ?: null;
}

function require_project(int $userId): array {
    $row = project_row($userId, (string) ($_GET['p'] ?? ''));
    if (!$row) {
        fail('No such project here yet', 404);
    }
    return $row;
}

function scene_id(): string {
    $id = (string) ($_GET['id'] ?? '');
    if (!preg_match('/^[A-Za-z0-9_-]{1,64}$/', $id)) {
        fail('That scene id is not valid', 400);
    }
    return $id;
}

function list_projects(int $userId): array {
    $stmt = db()->prepare('SELECT p.uid, p.title, UNIX_TIMESTAMP(p.updated_at) AS updated,
                                  (SELECT COUNT(*) FROM {scenes} s WHERE s.project_id = p.id
                                     AND s.scene_id NOT LIKE \'%-notes\' AND s.scene_id <> \'notepad\') AS scenes
                           FROM {projects} p WHERE p.user_id = ? ORDER BY p.updated_at DESC');
    $stmt->execute([$userId]);
    return array_map(fn($p) => [
        'id' => $p['uid'], 'title' => $p['title'], 'scenes' => (int) $p['scenes'], 'updatedAt' => (int) $p['updated'] * 1000,
    ], $stmt->fetchAll());
}

// Writer sends the list of scenes marked for the web (titles, chapters,
// synopses…). The helper keeps it as given for the web editor to show.
function put_index(int $userId, array $b): array {
    $title = mb_substr(trim((string) ($b['title'] ?? '')), 0, 200);
    if (!is_array($b['index'] ?? null)) {
        fail('index must be a list', 400);
    }
    $project = project_row($userId, (string) ($_GET['p'] ?? ''), true, $title);
    $before = stored_index($project);
    $stored = [
        'scenes' => array_values($b['index']),
        'stats' => is_array($b['stats'] ?? null) ? $b['stats'] : null,
        'syncedAt' => now_ms(),
        // The Mac counted what it had brought in by its last sync.
        'countedFrom' => $before['syncedAt'],
    ];
    db()->prepare('UPDATE {projects} SET title = ?, index_json = ? WHERE id = ?')
        ->execute([$title, json_encode($stored), $project['id']]);
    return ['ok' => true, 'seq' => (int) $project['seq']];
}

function now_ms(): int {
    return (int) round(microtime(true) * 1000);
}

// The index as kept: once a plain list of scenes, now with the day's writing.
function stored_index(array $project): array {
    $kept = json_decode($project['index_json'], true) ?: [];
    if (!array_key_exists('scenes', $kept)) {
        $kept = ['scenes' => array_values($kept)];  // the old shape: just the list
    }
    return [
        'scenes' => $kept['scenes'] ?? [],
        'stats' => $kept['stats'] ?? null,
        'syncedAt' => $kept['syncedAt'] ?? null,
        'countedFrom' => $kept['countedFrom'] ?? null,
    ];
}

function get_index(array $project): array {
    $kept = stored_index($project);
    return [
        'title' => $project['title'], 'index' => $kept['scenes'], 'seq' => (int) $project['seq'],
        'stats' => $kept['stats'], 'syncedAt' => $kept['syncedAt'], 'countedFrom' => $kept['countedFrom'],
    ];
}

function get_scene(array $project, string $id): array {
    $stmt = db()->prepare('SELECT text, version FROM {scenes} WHERE project_id = ? AND scene_id = ?');
    $stmt->execute([$project['id'], $id]);
    $row = $stmt->fetch();
    if (!$row) {
        fail('No such scene here yet', 404);
    }
    return ['id' => $id, 'text' => $row['text'], 'version' => (int) $row['version']];
}

// Save a scene, if it's still at the version the text was edited from. The
// version before is kept in scene_versions. The check and the save happen
// in one transaction, so two saves at once can't both win.
function put_scene(array $user, array $project, string $id, array $b): array {
    if (!is_string($b['text'] ?? null) || !is_int($b['base'] ?? null)) {
        fail('Send the text and the version it was edited from (base)', 400);
    }
    $pdo = db();
    $pdo->beginTransaction();
    $stmt = $pdo->prepare('SELECT text, version, saved_by, updated_at FROM {scenes} WHERE project_id = ? AND scene_id = ? FOR UPDATE');
    $stmt->execute([$project['id'], $id]);
    $now = $stmt->fetch();
    $current = $now ? (int) $now['version'] : 0;
    if ($current !== $b['base']) {
        $pdo->rollBack();
        fail('This scene was changed since you started editing it', 409, [
            'current' => $now ? ['text' => $now['text'], 'version' => $current] : null,
        ]);
    }
    if ($now && $now['text'] === $b['text']) {
        $pdo->rollBack();  // nothing new
        return ['version' => $current, 'seq' => (int) $project['seq'], 'savedAt' => now_ms()];
    }
    $pdo->prepare('UPDATE {projects} SET seq = seq + 1 WHERE id = ?')->execute([$project['id']]);
    $seq = (int) $pdo->query('SELECT seq FROM {projects} WHERE id = ' . (int) $project['id'])->fetchColumn();
    if ($now) {
        $pdo->prepare('INSERT INTO {scene_versions} (project_id, scene_id, version, text, saved_by, saved_at) VALUES (?, ?, ?, ?, ?, ?)')
            ->execute([$project['id'], $id, $current, $now['text'], $now['saved_by'], $now['updated_at']]);
        $pdo->prepare('UPDATE {scenes} SET text = ?, version = ?, seq = ?, saved_by = ? WHERE project_id = ? AND scene_id = ?')
            ->execute([$b['text'], $current + 1, $seq, $user['by'], $project['id'], $id]);
    } else {
        $pdo->prepare('INSERT INTO {scenes} (project_id, scene_id, text, version, seq, saved_by) VALUES (?, ?, ?, 1, ?, ?)')
            ->execute([$project['id'], $id, $b['text'], $seq, $user['by']]);
    }
    $pdo->commit();
    return ['version' => $current + 1, 'seq' => $seq, 'savedAt' => now_ms()];
}

// Scenes saved since sync point $since, text and all (scenes are small).
function changes(array $project, int $since): array {
    $stmt = db()->prepare('SELECT scene_id, text, version, saved_by, UNIX_TIMESTAMP(updated_at) AS updated
                           FROM {scenes} WHERE project_id = ? AND seq > ? ORDER BY seq');
    $stmt->execute([$project['id'], $since]);
    return ['seq' => (int) $project['seq'], 'scenes' => array_map(fn($s) => [
        'id' => $s['scene_id'], 'text' => $s['text'], 'version' => (int) $s['version'],
        'by' => $s['saved_by'], 'updatedAt' => (int) $s['updated'] * 1000,
    ], $stmt->fetchAll())];
}

function history(array $project, string $id): array {
    $stmt = db()->prepare('SELECT version, saved_by, UNIX_TIMESTAMP(saved_at) AS saved, text
                           FROM {scene_versions} WHERE project_id = ? AND scene_id = ? ORDER BY version DESC');
    $stmt->execute([$project['id'], $id]);
    return array_map(fn($v) => [
        'version' => (int) $v['version'], 'by' => $v['saved_by'], 'savedAt' => (int) $v['saved'] * 1000,
        'words' => str_word_count(strip_tags($v['text'])),
    ], $stmt->fetchAll());
}

function history_text(array $project, string $id, int $version): array {
    $stmt = db()->prepare('SELECT text FROM {scene_versions} WHERE project_id = ? AND scene_id = ? AND version = ?');
    $stmt->execute([$project['id'], $id, $version]);
    $text = $stmt->fetchColumn();
    if ($text === false) {
        fail('No such version', 404);
    }
    return ['text' => $text];
}

// A new scene from the page. With a key (the page's own name for it), sending
// it again (its reply lost on a bad connection, say) gives back the one
// already here rather than making another.
function inbox_add(array $project, array $b): array {
    $title = mb_substr(trim((string) ($b['title'] ?? '')), 0, 200) ?: 'Untitled';
    if (!is_string($b['text'] ?? null)) {
        fail('Send the text', 400);
    }
    $key = isset($b['key']) ? (string) $b['key'] : null;
    if ($key !== null && !preg_match('/^[A-Za-z0-9-]{8,64}$/', $key)) {
        fail('key must be 8 to 64 letters, digits or -', 400);
    }
    $already = function () use ($project, $key) {
        $stmt = db()->prepare('SELECT id FROM {inbox} WHERE project_id = ? AND client_key = ?');
        $stmt->execute([$project['id'], $key]);
        return $stmt->fetchColumn();
    };
    if ($key !== null && ($id = $already()) !== false) {
        return ['id' => (int) $id];
    }
    try {
        db()->prepare('INSERT INTO {inbox} (project_id, title, text, client_key) VALUES (?, ?, ?, ?)')
            ->execute([$project['id'], $title, $b['text'], $key]);
    } catch (PDOException $e) {
        // Sent twice at once: the other got in first.
        if ($key !== null && $e->getCode() === '23000' && ($id = $already()) !== false) {
            return ['id' => (int) $id];
        }
        throw $e;
    }
    return ['id' => (int) db()->lastInsertId()];
}

function inbox_list(array $project): array {
    $stmt = db()->prepare('SELECT id, title, text, UNIX_TIMESTAMP(created_at) AS created FROM {inbox}
                           WHERE project_id = ? AND NOT collected ORDER BY id');
    $stmt->execute([$project['id']]);
    return array_map(fn($i) => [
        'id' => (int) $i['id'], 'title' => $i['title'], 'text' => $i['text'], 'createdAt' => (int) $i['created'] * 1000,
    ], $stmt->fetchAll());
}

function inbox_collect(array $project, array $b): array {
    $ids = array_values(array_filter(array_map('intval', (array) ($b['ids'] ?? []))));
    if ($ids) {
        $marks = implode(',', array_fill(0, count($ids), '?'));
        db()->prepare("UPDATE {inbox} SET collected = TRUE WHERE project_id = ? AND id IN ($marks)")
            ->execute(array_merge([$project['id']], $ids));
    }
    return ['ok' => true];
}

// --- Routing ---

$method = $_SERVER['REQUEST_METHOD'];
$action = $_GET['action'] ?? '';

if ($method === 'GET' && $action === 'me') {
    send_json(me());
} elseif ($method === 'POST' && $action === 'setup') {
    send_json(setup(read_json()));
} elseif ($method === 'POST' && $action === 'login') {
    send_json(login(read_json()));
} elseif ($method === 'POST' && $action === 'logout') {
    send_json(logout());
} elseif ($method === 'POST' && $action === 'recover') {
    send_json(recover(read_json()));
}

$user = require_user();
$userId = $user['id'];

// Tokens are managed from a signed-in browser, not with a token.
$tokenActions = ['tokens', 'token_create', 'token_delete', 'change_password'];
if (in_array($action, $tokenActions, true) && $user['by'] !== BY_WEB) {
    fail('Sign in to the helper in a browser to manage tokens', 403);
}

if ($method === 'POST' && $action === 'change_password') {
    send_json(change_password($user, read_json()));
} elseif ($method === 'GET' && $action === 'tokens') {
    send_json(list_tokens($userId));
} elseif ($method === 'POST' && $action === 'token_create') {
    send_json(create_token($userId, read_json()));
} elseif ($method === 'POST' && $action === 'token_delete') {
    send_json(delete_token($userId, (int) ($_GET['id'] ?? 0)));
} elseif ($method === 'GET' && $action === 'projects') {
    send_json(list_projects($userId));
} elseif ($method === 'POST' && $action === 'index_put') {
    send_json(put_index($userId, read_json()));
} elseif ($method === 'GET' && $action === 'index') {
    send_json(get_index(require_project($userId)));
} elseif ($method === 'GET' && $action === 'scene') {
    send_json(get_scene(require_project($userId), scene_id()));
} elseif ($method === 'POST' && $action === 'scene_put') {
    send_json(put_scene($user, require_project($userId), scene_id(), read_json()));
} elseif ($method === 'GET' && $action === 'changes') {
    send_json(changes(require_project($userId), (int) ($_GET['since'] ?? 0)));
} elseif ($method === 'GET' && $action === 'history') {
    send_json(history(require_project($userId), scene_id()));
} elseif ($method === 'GET' && $action === 'history_text') {
    send_json(history_text(require_project($userId), scene_id(), (int) ($_GET['version'] ?? 0)));
} elseif ($method === 'POST' && $action === 'inbox_add') {
    send_json(inbox_add(require_project($userId), read_json()));
} elseif ($method === 'GET' && $action === 'inbox') {
    send_json(inbox_list(require_project($userId)));
} elseif ($method === 'POST' && $action === 'inbox_collect') {
    send_json(inbox_collect(require_project($userId), read_json()));
} else {
    fail('Unknown request', 404);
}
