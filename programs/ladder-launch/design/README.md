# Ladder Launch UI design

Source artboards for the launchpad front end. The live, clickable canvas is at
https://claude.ai/artifact/S9CMLC2X3cziyvuVggRWY5 (private; share from the page).

Each `*.dc.html` is one artboard: plain HTML with inline styles, a `<helmet>` for
fonts, and a small component script (`renderVals`) that fills `{{placeholders}}`,
`sc-for` lists and `sc-if` blocks. `support.js` is the canvas runtime and is not
checked in; the files are meant to be read as the spec for the real app, not run
as-is.

| Board | Size | What it is |
|---|---|---|
| `Main.dc.html` | 1280×920 | Launch page: chart, live tape, one-tap seat tiles, your seats |
| `Exit.dc.html` | 480×600 | Cash-out sheet (two taps, then the green success state) |
| `Create.dc.html` | 1280×760 | Create a launch: any SPL / Token-2022 quote, opening mcap, floor |
| `PhoneList.dc.html` | 390×844 | Phone: Hot / New / Mine list with heat bars |
| `Phone.dc.html` | 390×1100 | Phone: launch page |
| `PhoneExit.dc.html` | 390×844 | Phone: cash-out sheet |
| `PhoneCreate.dc.html` | 390×1240 | Phone: create a launch |

`canvas.json` holds board positions and the design notes.

## Rules

- **Fast fast dopamine fast.** Seat tiles (0.5 / 1 / 5 quote) are one tap, no confirm,
  no amount field; custom amount is tucked away. Every action ends in a green pop:
  `SEATED · #149`, or `1.42 SOL in your wallet`.
- **Numbers shout, copy whispers.** Market cap and the change figure are the biggest
  things on the page; each seat card leads with its gain. Explanations are one line
  behind "How it works".
- **Always moving.** Blinking LIVE dot, HOT chip with heat bar, scrolling tape of the
  last seats and exits, exit-room reset countdown.
- **Chart is an embed.** The chart slot is the GeckoTerminal / CoinGecko trading-terminal
  iframe for the Whirlpool pool address. Nothing custom to build.
- **Mobile.** One column, 16 px gutters, 44 px minimum targets, primary button above the
  thumb zone, sheets instead of modals, Mobile Wallet Adapter or in-app-browser deeplinks.

## Palette and type

| Token | Value |
|---|---|
| bg / panel / border | `#0f1214` / `#12161a` / `#22282c` |
| text / muted | `#e9edf0` / `#93a0a8` |
| accent (gain, seat) | `#2fd27a` |
| exit / cash out | `#f0a0a0` |
| heat (HOT chip and bars only) | `#ff8a3d` |
| display / body / numbers | Archivo 800 / IBM Plex Sans / IBM Plex Mono |
