# Electron Desktop Icons

Place desktop icon files here. electron-builder reads this directory as `buildResources`.

## Required files

| File | Min size | Platform |
|------|----------|----------|
| `icon.icns` | 512×512 | macOS (dmg / app bundle) |
| `icon.ico` | 256×256 | Windows (NSIS installer) |
| `icons/16x16.png` | 16×16 | Linux |
| `icons/32x32.png` | 32×32 | Linux |
| `icons/48x48.png` | 48×48 | Linux |
| `icons/64x64.png` | 64×64 | Linux |
| `icons/128x128.png` | 128×128 | Linux |
| `icons/256x256.png` | 256×256 | Linux |
| `icons/512x512.png` | 512×512 | Linux |

## Tips

- Start from a single 1024×1024 master PNG.
- Convert to `.icns` on macOS: `iconutil -c icns Icon.iconset`
- Convert to `.ico` with ImageMagick: `convert icon.png -resize 256x256 icon.ico`
- Linux PNGs can be exported from the master PNG at each size via ImageMagick or Inkscape.
