# Capacitor App Assets

Place source images here before running `npm run assets:generate`.
`@capacitor/assets` will generate all iOS, Android, and PWA icon/splash variants from these files.

## Required source files

| File | Min size | Purpose |
|------|----------|---------|
| `icon-only.png` | 1024×1024 | App icon (no background) |
| `icon-foreground.png` | 1024×1024 | Adaptive icon foreground layer |
| `icon-background.png` | 1024×1024 | Adaptive icon background layer |
| `splash.png` | 2732×2732 | Light-mode splash screen |
| `splash-dark.png` | 2732×2732 | Dark-mode splash screen |

## Generate

```bash
npm run assets:generate
```

This outputs into `ios/App/App/Assets.xcassets`, `android/app/src/main/res`, and `public/`.
