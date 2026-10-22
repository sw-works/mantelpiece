# Testing Mantelpiece

For hackathon judges. The hosted backend is up through **2026-11-20**, the end of judging.
Nothing needs an account: a Fire TV names its household on first launch and phones pair by
QR.

## What you need

- A Fire TV device in developer mode, or the Vega Virtual Device from the
  [Vega SDK](https://developer.amazon.com/docs/vega/0.24/install-vega-sdk.html) 0.24.
- A phone (or a desktop browser) for the phone pages.

## 1. Install the TV app

Download the `.vpkg` for your device from the
[judging release](https://github.com/sw-works/mantelpiece/releases/tag/judging):
`tv-vega_aarch64.vpkg` for the Virtual Device on an Apple silicon Mac, `tv-vega_x86_64.vpkg`
on an Intel machine, and on a Fire TV the one that matches its CPU (`armv7` or `aarch64`).
Each is a release build that talks to the judging backend.

```sh
vega virtual-device start                       # skip on a Fire TV
vega run-app tv-vega_aarch64.vpkg com.hanyu.mantelpiece.main -d VirtualDevice
```

On a Fire TV use its device id from `vega device list` in place of `VirtualDevice`.

## 2. First launch and pairing

1. The TV asks for a household name (it starts as the TV's own name). Select **Continue**.
2. Scan the QR on the TV with your phone, or open the link under it. Type your name.
3. Your name shows up on the TV. Select **Allow**. The phone is now paired; later phones pair
   from **Add photos** on Memories home.

## 3. See a finished trip right away

A demo household holds an eight-day trip around Iceland, June 27 to July 4, 2015 (photos by
Sergei Gussev, CC BY 2.0, from Wikimedia Commons).

1. On Memories home select **Join a trip** and enter the code **GJSB8X**.
2. Open the trip. Trip view shows the sketch map of the whole trip, each day with its own
   map, the day-by-day story and the highlights.
3. Select **Play**. Each new day opens on its map for a few seconds. Select **Field notes**
   to show the small maps inside the photos; with a field note on screen, press Select to
   open **This place**.
4. Back in Trip view, select **Play reel** for the trip as a music video, or **Day film**
   on a day.

## 4. Your own photos

1. On the phone page, pick photos. Photos taken on two or more consecutive days, with
   location on, make a trip; single days stay in the archive.
2. The TV shows each photo as it lands, with your name. Select **Love it**; the phone shows
   the notification (allow notifications when the page asks).
3. A few minutes after the last upload the trip appears on Memories home, with its story,
   maps and highlights. Field notes and sketch maps follow within a few more minutes.

## 5. Live from the phone

1. On the phone page choose **Go live** and allow the camera.
2. The TV shows "<name> wants to go live". Select **Watch**. The camera plays on the TV a few
   seconds behind.
3. **End broadcast** on the TV ends it on the phone too.

## Known limits

- Everything was run on the Vega Virtual Device; a physical Fire TV has not been tried.
- On the Virtual Device, reel music stops after about half a second and the reel carries on
  silently. Day films play, but slowly, since the emulator runs well below real time.
- Back from Trip view or a reel can leave the app for the launcher; relaunch it.
- Alexa+ and the Echo Show skill are built but not registered (`docs/alexa-plus.md`).
