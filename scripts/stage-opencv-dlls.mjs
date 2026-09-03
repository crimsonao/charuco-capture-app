import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const DEFAULT_OPENCV_DIR =
  'C:\\Users\\50429\\Desktop\\mark\\centerExtration\\opencv\\build';

function isBundledDll(name) {
  const lower = name.toLowerCase();
  if (!lower.endsWith('.dll')) {
    return false;
  }
  if (lower.startsWith('opencv_world')) {
    return !lower.endsWith('d.dll');
  }
  return lower.startsWith('opencv_videoio');
}

const opencvDir = process.env.OPENCV_DIR || DEFAULT_OPENCV_DIR;
const binDir = path.join(opencvDir, 'x64', 'vc16', 'bin');
const destDir = path.join('src-tauri', 'opencv-runtime');

if (!fs.existsSync(binDir)) {
  console.error(`OpenCV bin not found: ${binDir}`);
  console.error('Set OPENCV_DIR to the directory that contains include\\ and x64\\vc16\\');
  process.exit(1);
}

fs.mkdirSync(destDir, { recursive: true });

const names = fs.readdirSync(binDir).filter(isBundledDll);
const world = names.filter((name) => name.toLowerCase().startsWith('opencv_world'));
if (world.length === 0) {
  console.error(`No opencv_world*.dll in ${binDir}`);
  process.exit(1);
}

for (const name of names) {
  fs.copyFileSync(path.join(binDir, name), path.join(destDir, name));
  console.log(`staged ${name}`);
}
