/* Plank Voice Stopwatch
 * Starts the clock on "up"/"start" and stops on "down"/"stop",
 * using the browser's Web Speech API for recognition.
 */

(function () {
  "use strict";

  // ---- Stopwatch state ----
  var running = false;
  var startTimestamp = 0; // performance.now() when the current run started
  var accumulated = 0; // ms carried over (always 0 here; we reset on each run)
  var rafId = null;
  var laps = []; // completed plank durations in ms

  // ---- Voice command vocabulary ----
  var START_WORDS = ["up", "start", "go", "begin"];
  var STOP_WORDS = ["down", "stop", "rest", "end"];

  // ---- DOM ----
  var timeEl = document.getElementById("time");
  var statusEl = document.getElementById("status");
  var micBtn = document.getElementById("mic");
  var resetBtn = document.getElementById("reset");
  var lapsEl = document.getElementById("laps");
  var lapCountEl = document.getElementById("lapCount");
  var lapsSummaryEl = document.getElementById("lapsSummary");
  var heardEl = document.getElementById("heard");

  // ---- Time formatting ----
  function elapsedMs() {
    if (!running) {
      return accumulated;
    }
    return accumulated + (performance.now() - startTimestamp);
  }

  function formatTime(ms) {
    var totalTenths = Math.floor(ms / 100);
    var tenths = totalTenths % 10;
    var totalSeconds = Math.floor(totalTenths / 10);
    var seconds = totalSeconds % 60;
    var minutes = Math.floor(totalSeconds / 60);
    return (
      pad2(minutes) + ":" + pad2(seconds) + "." + tenths
    );
  }

  function pad2(n) {
    return n < 10 ? "0" + n : String(n);
  }

  function render() {
    timeEl.textContent = formatTime(elapsedMs());
    if (running) {
      rafId = requestAnimationFrame(render);
    }
  }

  // ---- Stopwatch controls ----
  function startClock() {
    if (running) {
      return;
    }
    running = true;
    accumulated = 0;
    startTimestamp = performance.now();
    timeEl.classList.add("running");
    setStatus('Counting… say <b>"down"</b> to stop.');
    render();
  }

  function stopClock() {
    if (!running) {
      return;
    }
    var duration = elapsedMs();
    running = false;
    accumulated = duration;
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
    timeEl.classList.remove("running");
    timeEl.textContent = formatTime(duration);
    recordLap(duration);
    setStatus('Nice hold! Say <b>"up"</b> to go again.');
  }

  function resetClock() {
    running = false;
    accumulated = 0;
    if (rafId !== null) {
      cancelAnimationFrame(rafId);
      rafId = null;
    }
    timeEl.classList.remove("running");
    timeEl.textContent = formatTime(0);
    laps = [];
    renderLaps();
    setStatus('Reset. Say <b>"up"</b> to start.');
  }

  // ---- Laps ----
  function recordLap(ms) {
    laps.push(ms);
    renderLaps();
  }

  function renderLaps() {
    lapsEl.innerHTML = "";
    laps.forEach(function (ms, i) {
      var li = document.createElement("li");
      var idx = document.createElement("span");
      idx.className = "lap-index";
      idx.textContent = "#" + (i + 1);
      var val = document.createElement("span");
      val.textContent = formatTime(ms);
      li.appendChild(idx);
      li.appendChild(val);
      lapsEl.appendChild(li);
    });

    if (laps.length === 0) {
      lapCountEl.textContent = "";
      lapsSummaryEl.textContent = "";
      return;
    }

    lapCountEl.textContent = "(" + laps.length + ")";
    var total = laps.reduce(function (a, b) {
      return a + b;
    }, 0);
    var best = laps.reduce(function (a, b) {
      return Math.max(a, b);
    }, 0);
    lapsSummaryEl.textContent =
      "Total " + formatTime(total) + " · Best " + formatTime(best);
  }

  // ---- Status helper ----
  function setStatus(html, isError) {
    statusEl.innerHTML = html;
    statusEl.classList.toggle("error", !!isError);
  }

  // ---- Command matching ----
  // Matches whole words so "startled" won't trigger "start".
  function containsWord(transcript, words) {
    var tokens = transcript.toLowerCase().split(/[^a-z]+/);
    for (var i = 0; i < words.length; i++) {
      if (tokens.indexOf(words[i]) !== -1) {
        return true;
      }
    }
    return false;
  }

  function handleTranscript(transcript) {
    heardEl.textContent = "heard: " + transcript;
    var wantsStart = containsWord(transcript, START_WORDS);
    var wantsStop = containsWord(transcript, STOP_WORDS);

    // If both appear in one utterance, act on current state sensibly.
    if (wantsStart && wantsStop) {
      if (running) {
        stopClock();
      } else {
        startClock();
      }
      return;
    }
    if (wantsStart) {
      startClock();
    } else if (wantsStop) {
      stopClock();
    }
  }

  // ---- Speech recognition ----
  var SpeechRecognition =
    window.SpeechRecognition || window.webkitSpeechRecognition;
  var recognition = null;
  var listening = false;
  var wantListening = false; // user intent; used to auto-restart

  function setupRecognition() {
    if (!SpeechRecognition) {
      micBtn.disabled = true;
      setStatus(
        "Voice recognition isn't supported in this browser. Try Chrome, Edge, or Safari.",
        true
      );
      return false;
    }
    recognition = new SpeechRecognition();
    recognition.continuous = true;
    recognition.interimResults = true;
    recognition.lang = "en-US";

    recognition.onresult = function (event) {
      for (var i = event.resultIndex; i < event.results.length; i++) {
        var transcript = event.results[i][0].transcript.trim();
        if (transcript) {
          handleTranscript(transcript);
        }
      }
    };

    recognition.onerror = function (event) {
      if (event.error === "not-allowed" || event.error === "service-not-allowed") {
        wantListening = false;
        setMicUI(false);
        setStatus(
          "Microphone access was blocked. Allow mic permission and tap “Start Listening” again.",
          true
        );
      } else if (event.error === "no-speech") {
        // Benign; recognition will be restarted by onend.
      } else if (event.error === "aborted") {
        // Usually from our own stop; ignore.
      } else {
        setStatus("Speech error: " + event.error, true);
      }
    };

    recognition.onend = function () {
      listening = false;
      // The API stops on its own periodically; restart if the user
      // still wants to be listening.
      if (wantListening) {
        try {
          recognition.start();
          listening = true;
        } catch (e) {
          // start() throws if called too soon; retry shortly.
          setTimeout(function () {
            if (wantListening) {
              try {
                recognition.start();
                listening = true;
              } catch (e2) {
                /* give up silently; user can retap */
              }
            }
          }, 300);
        }
      } else {
        setMicUI(false);
      }
    };

    return true;
  }

  function startListening() {
    if (!recognition && !setupRecognition()) {
      return;
    }
    wantListening = true;
    try {
      recognition.start();
      listening = true;
      setMicUI(true);
      setStatus('Listening… say <b>"up"</b> to start.');
    } catch (e) {
      // Already started — fine.
      setMicUI(true);
    }
  }

  function stopListening() {
    wantListening = false;
    if (recognition) {
      recognition.stop();
    }
    setMicUI(false);
    setStatus("Stopped listening.");
  }

  function setMicUI(on) {
    micBtn.classList.toggle("listening", on);
    micBtn.setAttribute("aria-pressed", on ? "true" : "false");
    micBtn.textContent = on ? "⏹ Stop Listening" : "🎙️ Start Listening";
  }

  // ---- Wire up UI ----
  micBtn.addEventListener("click", function () {
    if (wantListening) {
      stopListening();
    } else {
      startListening();
    }
  });

  resetBtn.addEventListener("click", resetClock);

  // Keyboard fallback for testing without a mic:
  // Space = toggle clock, R = reset.
  document.addEventListener("keydown", function (e) {
    if (e.code === "Space") {
      e.preventDefault();
      if (running) {
        stopClock();
      } else {
        startClock();
      }
    } else if (e.key === "r" || e.key === "R") {
      resetClock();
    }
  });

  // Initial paint.
  timeEl.textContent = formatTime(0);
  if (!SpeechRecognition) {
    setupRecognition();
  }
})();
