<p align="center">
  <a href="https://www.buymeacoffee.com/vesdal">
    <img src="https://img.shields.io/badge/%F0%9F%8D%B5_Buy_me_a_matcha-f4e9c6?style=for-the-badge&labelColor=f4e9c6&color=f4e9c6" alt="🍵 Buy me a matcha" width="420">
  </a>
</p>

# Skim-Android

Hi

[Skim](https://github.com/nikserg/skim) is a fast, minimalist, AI-native email
client - but desktop-only. I wanted it on my phone (Galaxy S25 Ultra), so I
had it adapted to Android: full-screen edge-to-edge UI, touch layouts, system
back button, pull-to-refresh, long-press multi-select, native notification
permission, and the same Thunderbird OAuth login for Gmail and Outlook so institutional account's won't piss u off.

This is my 2nd experiment, built with AI; so again, have mercy lol.

## Money bit (copy and paste from my other project, I'm sry but I need some rest fr)

Built with AI assistance because I have neither coding skills nor time to learn
(uni is graping my ahh). Charging like I wrote all this myself wouldn't feel
right, so it's free and here on github (the goat, long live the admins).

Any support is appreciated tho.

## What's included

- Full Skim source (Svelte frontend + Rust backend via Tauri), based on
  upstream [`nikserg/skim`](https://github.com/nikserg/skim) v1.0.31, adapted
  for Android (arm64). A signed APK is in `build-android/`.
- Thunderbird OAuth login kept as-is: Google and Microsoft sign-in use
  Thunderbird's public OAuth credentials (same ones shipped in every
  Thunderbird copy), so no app passwords needed.
- Android work: edge-to-edge + cutout handling, 16 KiB-page native lib,
  adaptive single/dual pane, drawer with account switcher, notification
  permission flow, predictive back, OAuth env baked at build time.

## Building it yourself

You need the Android SDK + NDK, Rust Android targets, and Node. Then:

```sh
npm install
npm run build
```

Set the Thunderbird OAuth credentials (public values from
[Mozilla's source](https://github.com/mozilla/releases-comm-central/blob/master/mailnews/base/src/OAuth2Providers.sys.mjs))
as env vars before the Android build:

```sh
SKIM_GOOGLE_CLIENT_ID=406964657835-aq8lmia8j95dhl1a2bvharmfk3t1hgqj.apps.googleusercontent.com
SKIM_GOOGLE_CLIENT_SECRET=kSmqreRr0qwBWJgbf5Y-PjSU
SKIM_MICROSOFT_CLIENT_ID=9e5f94bc-e8a4-4e73-b8be-63364c29d753
SKIM_GOOGLE_OAUTH_VERIFIED=1
SKIM_MICROSOFT_OAUTH_VERIFIED=1
```

```sh
npx tauri android build --apk --target aarch64
```

Sign the resulting APK with your own keystore (`apksigner`) — Android refuses
updates signed with a different key, so keep it safe.

## Credits

**Original Skim developers**
[nikserg](https://github.com/nikserg) and
[all Skim contributors](https://github.com/nikserg/skim/graphs/contributors).

**Vesdal:** idea, direction, and feedback.
**AI coding assistants:** Android adaptation and iteration.

Thunderbird OAuth credentials belong to Mozilla Thunderbird (MPL-2.0 source,
linked above).
Not affiliated with Google, Microsoft, or Mozilla/Thunderbird.
