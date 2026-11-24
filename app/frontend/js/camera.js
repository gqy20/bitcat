(function () {
  const invoke = window.__TAURI__?.core?.invoke;
  const listen = window.__TAURI__?.event?.listen;
  const video = document.getElementById("camera-video");
  const canvas = document.getElementById("camera-canvas");
  let stream = null;
  let streamReady = false;
  let captureRunning = false;
  let permissionRetryAt = 0;
  let permissionNoticeAt = 0;
  const PERMISSION_RETRY_MS = 5 * 60 * 1000;

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

  // A denied camera permission will not recover on its own: every sampling
  // tick would otherwise re-prompt and re-log. Pause retries for 5 minutes.
  function isPermissionError(error) {
    const name = String(error?.name || "");
    const message = String(error?.message || error || "").toLowerCase();
    return (
      name === "NotAllowedError" ||
      name === "SecurityError" ||
      message.includes("permission denied") ||
      message.includes("permission dismissed") ||
      message.includes("not allowed")
    );
  }

  function permissionCoolingDown() {
    return Date.now() < permissionRetryAt;
  }

  function notePermissionError(context, error) {
    const now = Date.now();
    permissionRetryAt = now + PERMISSION_RETRY_MS;
    stopStream();
    if (now - permissionNoticeAt >= PERMISSION_RETRY_MS) {
      permissionNoticeAt = now;
      log(`${context}: camera permission denied; retry paused for 5 minutes`);
    }
  }

  async function loadSettings() {
    const snapshot = await invoke("cmd_settings_load");
    return {
      appearance: snapshot?.appearance || {},
      permissions: snapshot?.permissions || {},
    };
  }

  async function ensureStream() {
    if (stream) return stream;
    if (permissionCoolingDown()) return null;
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
      const { appearance, permissions } = await loadSettings();
      if (!appearance.camera_observation_enabled || !permissions.allow_camera_observation) {
        stopStream();
        return;
      }
      if (permissionCoolingDown()) return;
      await ensureStream();
      if (!streamReady) return;
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
      if (isPermissionError(error)) {
        notePermissionError("camera observation failed", error);
      } else {
        log(`camera observation failed: ${error?.message || error}`);
      }
    } finally {
      captureRunning = false;
    }
  }

  async function start() {
    if (permissionCoolingDown()) return;
    await ensureStream();
    if (!streamReady) return;
    log("camera observation stream armed");
  }

  async function refreshFromSettings() {
    try {
      const { appearance, permissions } = await loadSettings();
      if (appearance.camera_observation_enabled && permissions.allow_camera_observation) {
        if (permissionCoolingDown()) return;
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
      if (isPermissionError(error)) {
        notePermissionError("camera refresh failed", error);
      } else {
        log(`camera refresh failed: ${error?.message || error}`);
      }
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
