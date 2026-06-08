# Plank Voice Stopwatch

A tiny, zero-dependency web app that times your planks hands-free. Say **"up"**
or **"start"** to start the clock and **"down"** or **"stop"** to stop it — handy
when you're in plank position and can't reach your phone.

## Run it

It's a static page, so any of these work:

```bash
# Option 1: just open the file
open index.html            # macOS
xdg-open index.html        # Linux

# Option 2: serve it (recommended — some browsers gate the mic on file://)
python3 -m http.server 8000
# then visit http://localhost:8000
```

> **Microphone note:** Browsers only allow microphone access over `https://` or
> on `http://localhost`. If you open the raw file and the mic doesn't work, use
> the local server option above, or host it somewhere with HTTPS.

## Use it

1. Tap **Start Listening** and grant microphone permission.
2. Say **"up"** (or "start"/"go"/"begin") to start timing.
3. Hold your plank.
4. Say **"down"** (or "stop"/"rest"/"end") to stop. The hold is saved to the
   list below with a running total and your best time.

No mic handy? You can drive it with the keyboard for testing:

- **Space** — toggle the clock (start/stop)
- **R** — reset everything

## How it works

- **Stopwatch** uses `performance.now()` for drift-free timing and
  `requestAnimationFrame` for a smooth tenths-of-a-second display.
- **Voice** uses the browser's built-in
  [Web Speech API](https://developer.mozilla.org/en-US/docs/Web/API/SpeechRecognition)
  (`SpeechRecognition` / `webkitSpeechRecognition`) in continuous mode. Commands
  are matched on whole words, so "startled" won't trip "start". Recognition
  auto-restarts when the API ends a session so listening stays continuous.

## Browser support

Works best in **Chrome**, **Edge**, and **Safari** (including iOS Safari).
Firefox does not currently ship the Web Speech recognition API; the app detects
this and shows a message, and you can still use the keyboard controls.

## Files

| File         | Purpose                                  |
| ------------ | ---------------------------------------- |
| `index.html` | Markup and layout                        |
| `style.css`  | Styling (dark, mobile-friendly)          |
| `app.js`     | Stopwatch logic + voice command handling |
