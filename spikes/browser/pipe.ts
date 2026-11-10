import { chromium } from 'playwright-core';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdir, writeFile } from 'node:fs/promises';
const here=dirname(fileURLToPath(import.meta.url));
const out=resolve(here,'../../.spike/browser');
await mkdir(out,{recursive:true,mode:0o700});
const context=await chromium.launchPersistentContext(resolve(out,`pipe-${process.pid}`),{
  executablePath:'/usr/bin/chromium',headless:false,
  ignoreDefaultArgs:['--disable-extensions'],
  args:['--ozone-platform=wayland','--enable-unsafe-extension-debugging'],
});
try {
  const session=await context.browser()!.newBrowserCDPSession();
  const result=await session.send('Extensions.loadUnpacked',{path:resolve(out,'extension')});
  console.log(JSON.stringify({pipeLoadUnpacked:result}));
  await writeFile(resolve(out,'pipe-results.json'),JSON.stringify({pipeLoadUnpacked:result},null,2));
} finally {await context.close();}
