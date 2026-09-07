import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.resolve(__dirname, '..')

function isBundledDll(name) {
  const lower = name.toLowerCase()
  if (!lower.endsWith('.dll')) {
    return false
  }
  if (lower.startsWith('opencv_world')) {
    return !lower.endsWith('d.dll')
  }
  return lower.startsWith('opencv_videoio')
}

function readOpenCvDirFromDotenv() {
  const dotenvPath = path.join(repoRoot, '.env')
  if (!fs.existsSync(dotenvPath)) {
    return ''
  }
  const lines = fs.readFileSync(dotenvPath, 'utf8').split(/\r?\n/)
  for (const raw of lines) {
    const line = raw.trim()
    if (!line || line.startsWith('#')) {
      continue
    }
    const eq = line.indexOf('=')
    if (eq < 0) {
      continue
    }
    const key = line.slice(0, eq).trim()
    if (key !== 'OPENCV_DIR') {
      continue
    }
    return line
      .slice(eq + 1)
      .trim()
      .replace(/^['"]|['"]$/g, '')
  }
  return ''
}

const opencvDir = (process.env.OPENCV_DIR || readOpenCvDirFromDotenv()).trim()
if (!opencvDir) {
  console.error('OPENCV_DIR is not set.')
  console.error(
    'Set the environment variable, or put OPENCV_DIR=... in a gitignored repo-root .env (see .env.example).'
  )
  process.exit(1)
}

const binDir = path.join(opencvDir, 'x64', 'vc16', 'bin')
const destDir = path.join(repoRoot, 'src-tauri', 'opencv-runtime')

if (!fs.existsSync(binDir)) {
  console.error(`OpenCV bin not found: ${binDir}`)
  console.error('Check OPENCV_DIR points at the build root with include\\ and x64\\vc16\\')
  process.exit(1)
}

fs.mkdirSync(destDir, { recursive: true })

const names = fs.readdirSync(binDir).filter(isBundledDll)
const world = names.filter((name) => name.toLowerCase().startsWith('opencv_world'))
if (world.length === 0) {
  console.error(`No opencv_world*.dll in ${binDir}`)
  process.exit(1)
}

for (const name of names) {
  fs.copyFileSync(path.join(binDir, name), path.join(destDir, name))
  console.log(`staged ${name}`)
}
