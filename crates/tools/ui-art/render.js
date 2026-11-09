// Shoots the sprites of one ui-art run on a transparent background at the catalog's scale.
// make_ui_art.py writes a job file; each shot puts one sprite's SVG on an empty page (its CSS
// filter on the holder, for derived states), waits for it to paint, and screenshots it.
//
// Usage: node render.js JOB.json
// JOB: { "viewport": [w, h], "scale": 2,
//        "shots": [ { "svg": "<svg ...>", "css": "brightness(1.1)", "w": 336, "h": 216, "out": "s0.png" } ] }
//
// Needs Playwright with Chromium, installed outside the repository: `npm i -g playwright && npx playwright install chromium`
// (cloud sessions have both installed).
const fs = require('fs');
const { chromium } = require('playwright');

const PAGE = '<!doctype html><html><head><meta charset="utf-8"><style>'
  + 'html,body{margin:0;padding:0;background:transparent}svg{display:block}'
  + '#s{position:absolute;left:0;top:0}</style></head><body><div id="s"></div></body></html>';

(async () => {
  const job = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
  const browser = await chromium.launch();
  const page = await browser.newPage({
    viewport: { width: job.viewport[0], height: job.viewport[1] },
    deviceScaleFactor: job.scale,
  });
  await page.setContent(PAGE);
  for (const s of job.shots) {
    await page.evaluate(([svg, css]) => new Promise((done) => {
      const holder = document.getElementById('s');
      holder.innerHTML = svg;
      holder.style.filter = css;
      requestAnimationFrame(() => requestAnimationFrame(done));
    }), [s.svg, s.css]);
    await page.screenshot({ path: s.out, omitBackground: true, clip: { x: 0, y: 0, width: s.w, height: s.h } });
  }
  await browser.close();
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
