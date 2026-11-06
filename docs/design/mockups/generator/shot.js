// Screenshots build/<name>.html into ../<name>.jpg.
// Usage: node shot.js NAME... (env SCALE, default 2; QUALITY, default 85)
const { chromium } = require('playwright');
const path = require('path');
(async () => {
  const scale = Number(process.env.SCALE || 2);
  const quality = Number(process.env.QUALITY || 85);
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: scale });
  for (const name of process.argv.slice(2)) {
    await p.goto('file://' + path.join(__dirname, 'build', `${name}.html`));
    await p.evaluate(() => document.fonts.ready);
    await p.waitForTimeout(300);
    const out = path.join(process.env.OUTDIR || path.join(__dirname, '..'), `${name}.jpg`);
    await p.screenshot({ path: out, type: 'jpeg', quality });
    console.log('wrote', out);
  }
  await b.close();
})();
