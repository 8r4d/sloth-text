<?php
// The page. Served by PHP so each script's and stylesheet's address carries
// its file's modification time (app.js?v=1763841234): every upload is a new
// address, so Cloudflare and browsers never serve a stale copy, and there are
// no version numbers to raise by hand. PHP's own output isn't cached.
header('Cache-Control: no-cache');

function asset(string $file): string {
    return $file . '?v=' . (@filemtime(__DIR__ . '/' . $file) ?: 0);
}

// The service worker learns the page's files (and so when to refresh its
// copies) from its own address.
$worker = 'sw.js?' . http_build_query(['v' => @filemtime(__DIR__ . '/sw.js') ?: 0, 'app' => asset('app.js'), 'css' => asset('style.css')]);
$h = fn(string $s) => htmlspecialchars($s, ENT_QUOTES);
?>
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
  <title>Sloth Writer</title>
  <link rel="manifest" href="<?= $h(asset('manifest.json')) ?>">
  <link rel="apple-touch-icon" href="<?= $h(asset('apple-touch-icon.png')) ?>">
  <meta name="apple-mobile-web-app-capable" content="yes">
  <meta name="apple-mobile-web-app-title" content="Writer">
  <meta name="theme-color" content="#f6f1e7" media="(prefers-color-scheme: light)">
  <meta name="theme-color" content="#1d1b18" media="(prefers-color-scheme: dark)">
  <link rel="stylesheet" href="<?= $h(asset('style.css')) ?>">
</head>
<body>
  <main id="app">
    <h1>Sloth Writer</h1>
    <p class="quiet">Loading…</p>
  </main>
  <script>window.WORKER = <?= json_encode($worker) ?>;</script>
  <script src="<?= $h(asset('app.js')) ?>"></script>
</body>
</html>
