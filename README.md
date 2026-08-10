# Colorify

Colorify is a Windows desktop utility for adjusting display color settings from a clean local app.

It provides controls for brightness, contrast, saturation, hue, gamma, and color temperature, with saved presets, import/export, tray behavior, and optional startup settings.

## Why Colorify

Colorify applies color adjustments at the Windows desktop level, so the effect can be visible not only on your monitor, but also in normal screen recordings and screen captures.

That matters because many monitor OSD settings and some GPU control panel adjustments only affect what you personally see on the physical display. Colorify is built for users who want their tuned look to carry into recorded gameplay, desktop captures, and shared clips too.

## Download

Colorify is available on the Microsoft Store:

https://apps.microsoft.com/detail/9N4VW2LKQBTT

The Windows MSI installer is also available from the GitHub Releases page:

https://github.com/KMGY777/Colorify/releases

## Features

- Capture-visible screen color adjustments for recordings and shared clips.
- Adjust display color controls from one desktop app.
- Create, save, import, and export custom presets.
- Includes ready-made presets for visibility, gaming, vibrant color, and balanced everyday use.
- Runs from the Windows system tray.
- Optional start with Windows and start minimized behavior.
- Local-only design with no account, ads, analytics, or tracking.

## Development

Requirements:

- Node.js
- Rust
- Tauri CLI

Install dependencies:

```powershell
npm install
```

Run in development:

```powershell
npm run tauri:dev
```

Build:

```powershell
npm run tauri:build
```

## Privacy

Colorify does not collect, transmit, sell, rent, or share personal information. Settings are stored locally on the user's device.

See `PRIVACY_POLICY.txt` for details.

## License

Colorify is proprietary software. See `LICENSE.txt` and `TERMS.txt`.

Third-party notices are listed in `THIRD-PARTY-NOTICES.txt`.
