// 1x JPEG previews of build/<name>.html into build/prev-<name>.jpg, for checking layouts.
const { chromium } = require('playwright');
const path = require('path');
(async () => {
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1920, height: 1080 } });
  for (const n of process.argv.slice(2)) {
    await p.goto('file://' + path.join(__dirname, 'build', n + '.html'));
    await p.evaluate(() => document.fonts.ready);
    await p.waitForTimeout(200);
    await p.screenshot({ path: path.join(__dirname, 'build', 'prev-' + n + '.jpg'), type: 'jpeg', quality: 80 });
  }
  await b.close();
})();
