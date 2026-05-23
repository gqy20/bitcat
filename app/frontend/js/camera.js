(function () {
  const invoke = window.__TAURI__?.core?.invoke;
  const video = document.getElementById("camera-video");
  const canvas = document.getElementById("camera-canvas");
  let stream = null;
  let timer = null;
  let running = false;

  function log(message) {
    try {
      invoke?.("cmd_camera_log", { message: String(message) });
    } catch {}
  }

  function stopStream() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    if (stream) {
      for (const track of stream.getTracks()) track.stop();
      stream = null;
    }
    running = false;
  }

  async function loadSettings() {
    const snapshot = await invoke("cmd_settings_load");
    return snapshot?.appearance || {};
  }

  async function ensureStream() {
    if (stream) return stream;
    stream = await navigator.mediaDevices.getUserMedia({
      video: {
        width: { ideal: 640 },
        height: { ideal: 480 },
        facingMode: "user",
      },
      audio: false,
    });
    video.srcObject = stream;
    await video.play();
    return stream;
  }

  async function waitForVideoReady() {
    if (video.videoWidth > 0 && video.videoHeight > 0) return;
    await new Promise((resolve) => {
      const done = () => {
        video.removeEventListener("loadedmetadata", done);
        resolve();
      };
      video.addEventListener("loadedmetadata", done, { once: true });
      setTimeout(done, 1500);
    });
  }

  async function captureOnce() {
    await ensureStream();
    await waitForVideoReady();
    const width = video.videoWidth || 640;
    const height = video.videoHeight || 480;
    canvas.width = width;
    canvas.height = height;
    const ctx = canvas.getContext("2d", { willReadFrequently: false });
    ctx.drawImage(video, 0, 0, width, height);
    const dataUrl = canvas.toDataURL("image/jpeg", 0.78);
    await invoke("cmd_camera_frame", { dataUrl, width, height });
  }

  async function loop() {
    if (!running) return;
    let interval = 300;
    try {
      const appearance = await loadSettings();
      if (!appearance.camera_observation_enabled) {
        stopStream();
        return;
      }
      interval = Math.min(3600, Math.max(60, appearance.camera_observation_interval_sec || 300));
      await captureOnce();
    } catch (error) {
      log(`camera observation failed: ${error?.message || error}`);
    } finally {
      if (running) {
        timer = setTimeout(loop, interval * 1000);
      }
    }
  }

  async function start() {
    if (running) return;
    running = true;
    await loop();
  }

  window.addEventListener("camera-observation-refresh", async () => {
    try {
      const appearance = await loadSettings();
      if (appearance.camera_observation_enabled) {
        await start();
      } else {
        stopStream();
      }
    } catch (error) {
      log(`camera refresh failed: ${error?.message || error}`);
    }
  });

  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) {
      window.dispatchEvent(new Event("camera-observation-refresh"));
    }
  });

  window.addEventListener("beforeunload", stopStream);
  window.dispatchEvent(new Event("camera-observation-refresh"));
})();
