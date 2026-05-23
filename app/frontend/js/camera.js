(function () {
  const invoke = window.__TAURI__?.core?.invoke;
  const listen = window.__TAURI__?.event?.listen;
  const video = document.getElementById("camera-video");
  const canvas = document.getElementById("camera-canvas");
  let stream = null;
  let streamReady = false;
  let captureRunning = false;

  function log(message) {
    try {
      invoke?.("cmd_camera_log", { message: String(message) });
    } catch {}
  }

  function stopStream() {
    if (stream) {
      for (const track of stream.getTracks()) track.stop();
      stream = null;
    }
    streamReady = false;
    log("camera stream stopped");
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
    streamReady = true;
    log("camera stream started");
    try {
      await invoke("cmd_camera_ready");
    } catch (error) {
      log(`camera ready hide failed: ${error?.message || error}`);
    }
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
    if (captureRunning) {
      log("camera capture skipped because previous capture is running");
      return;
    }
    captureRunning = true;
    try {
      const appearance = await loadSettings();
      if (!appearance.camera_observation_enabled) {
        stopStream();
        return;
      }
      await ensureStream();
      await waitForVideoReady();
      const width = video.videoWidth || 640;
      const height = video.videoHeight || 480;
      canvas.width = width;
      canvas.height = height;
      const ctx = canvas.getContext("2d", { willReadFrequently: false });
      ctx.drawImage(video, 0, 0, width, height);
      const dataUrl = canvas.toDataURL("image/jpeg", 0.78);
      log(`camera frame captured ${width}x${height}`);
      await invoke("cmd_camera_frame", { dataUrl, width, height });
    } catch (error) {
      log(`camera observation failed: ${error?.message || error}`);
    } finally {
      captureRunning = false;
    }
  }

  async function start() {
    await ensureStream();
    log("camera observation stream armed");
  }

  async function refreshFromSettings() {
    try {
      const appearance = await loadSettings();
      if (appearance.camera_observation_enabled) {
        if (streamReady) {
          try {
            await invoke("cmd_camera_ready");
          } catch {}
        }
        await start();
      } else {
        stopStream();
      }
    } catch (error) {
      log(`camera refresh failed: ${error?.message || error}`);
    }
  }

  if (listen) {
    listen("camera-observation-refresh", refreshFromSettings)
      .then(() => log("camera refresh listener registered"))
      .catch((error) => log(`camera refresh listener failed: ${error?.message || error}`));
    listen("camera-observation-capture", captureOnce)
      .then(() => log("camera capture listener registered"))
      .catch((error) => log(`camera capture listener failed: ${error?.message || error}`));
  } else {
    log("tauri event API unavailable; using local refresh only");
    window.addEventListener("camera-observation-refresh", refreshFromSettings);
    window.addEventListener("camera-observation-capture", captureOnce);
  }

  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) {
      refreshFromSettings();
    }
  });

  window.addEventListener("beforeunload", stopStream);
  refreshFromSettings();
})();
