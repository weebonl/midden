(function () {
  const csrfCookie = "midden_csrf";
  const csrfField = "csrf_token";

  function readCookie(name) {
    return document.cookie
      .split(";")
      .map((part) => part.trim())
      .find((part) => part.startsWith(name + "="))
      ?.slice(name.length + 1);
  }

  function ensureCsrfField(form) {
    const token = readCookie(csrfCookie);
    if (!token || form.querySelector('input[name="' + csrfField + '"]')) return;
    const input = document.createElement("input");
    input.type = "hidden";
    input.name = csrfField;
    input.value = decodeURIComponent(token);
    form.appendChild(input);
  }

  document.querySelectorAll("form").forEach((form) => {
    if ((form.method || "").toLowerCase() === "post") {
      ensureCsrfField(form);
      form.addEventListener("submit", () => ensureCsrfField(form));
    }
  });

  function setupDropZone(dropZone, input) {
    ["dragenter", "dragover"].forEach((eventName) => {
      dropZone.addEventListener(eventName, (event) => {
        event.preventDefault();
        dropZone.classList.add("is-dragging");
      });
    });

    ["dragleave", "drop"].forEach((eventName) => {
      dropZone.addEventListener(eventName, (event) => {
        event.preventDefault();
        dropZone.classList.remove("is-dragging");
      });
    });

    dropZone.addEventListener("drop", (event) => {
      if (event.dataTransfer && event.dataTransfer.files.length > 0) {
        input.files = event.dataTransfer.files;
      }
    });
  }

  const form = document.querySelector("[data-upload-form]");
  const dropZone = document.querySelector("[data-drop-zone]");
  const input = dropZone ? dropZone.querySelector("input[type=file]") : null;
  const progress = document.querySelector("[data-upload-progress]");

  if (form && dropZone && input) {
    setupDropZone(dropZone, input);
    form.addEventListener("submit", (event) => {
      if (!progress || !window.XMLHttpRequest || !window.FormData) return;
      event.preventDefault();
      ensureCsrfField(form);
      progress.hidden = false;
      progress.value = 0;

      const request = new XMLHttpRequest();
      request.open("POST", form.action || window.location.href);
      request.upload.addEventListener("progress", (progressEvent) => {
        if (progressEvent.lengthComputable) {
          progress.value = Math.round((progressEvent.loaded / progressEvent.total) * 100);
        } else {
          progress.removeAttribute("value");
        }
      });
      request.addEventListener("load", () => {
        document.open();
        document.write(request.responseText);
        document.close();
      });
      request.addEventListener("error", () => {
        progress.removeAttribute("value");
        form.submit();
      });
      request.send(new FormData(form));
    });
  }

  const tusForm = document.querySelector("[data-tus-form]");
  if (!tusForm || !window.fetch || !window.Blob) return;

  const tusInput = tusForm.querySelector("input[type=file]");
  const tusDropZone = tusForm.querySelector(".drop-zone");
  const tusProgress = tusForm.querySelector("[data-tus-progress]");
  const tusStatus = tusForm.querySelector("[data-tus-status]");
  const tusButton = tusForm.querySelector("button[type=submit]");
  const chunkSize = 1024 * 1024;

  if (tusDropZone && tusInput) setupDropZone(tusDropZone, tusInput);

  function setTusStatus(message, isError) {
    if (!tusStatus) return;
    tusStatus.hidden = false;
    tusStatus.textContent = message;
    tusStatus.classList.toggle("error", Boolean(isError));
  }

  function metadataValue(value) {
    return btoa(unescape(encodeURIComponent(value)));
  }

  function uploadKey(file) {
    return "midden:tus:" + [file.name, file.size, file.lastModified].join(":");
  }

  function storedLocation(file) {
    try {
      return window.localStorage.getItem(uploadKey(file));
    } catch (_) {
      return null;
    }
  }

  function rememberLocation(file, location) {
    try {
      window.localStorage.setItem(uploadKey(file), location);
    } catch (_) {}
  }

  function forgetLocation(file) {
    try {
      window.localStorage.removeItem(uploadKey(file));
    } catch (_) {}
  }

  async function createTusUpload(file, expires) {
    const metadata = [
      "filename " + metadataValue(file.name || "upload.bin"),
      "content-type " + metadataValue(file.type || "application/octet-stream"),
    ];
    if (expires) metadata.push("expires " + metadataValue(expires));
    const visibility = tusForm.querySelector("select[name=visibility]")?.value;
    if (visibility) metadata.push("visibility " + metadataValue(visibility));
    const response = await fetch("/tus", {
      method: "POST",
      headers: {
        "Tus-Resumable": "1.0.0",
        "Upload-Length": String(file.size),
        "Upload-Metadata": metadata.join(","),
      },
    });
    if (!response.ok) throw new Error("Upload creation failed");
    return new URL(response.headers.get("location"), window.location.origin).toString();
  }

  async function currentTusOffset(location) {
    const response = await fetch(location, {
      method: "HEAD",
      headers: { "Tus-Resumable": "1.0.0" },
    });
    if (!response.ok) throw new Error("Upload resume failed");
    return Number(response.headers.get("upload-offset") || "0");
  }

  async function sendTusChunk(location, file, offset) {
    const chunk = file.slice(offset, Math.min(file.size, offset + chunkSize));
    const response = await fetch(location, {
      method: "PATCH",
      headers: {
        "Tus-Resumable": "1.0.0",
        "Upload-Offset": String(offset),
        "Content-Type": "application/offset+octet-stream",
      },
      body: chunk,
    });
    if (!response.ok) throw new Error("Upload chunk failed");
    return {
      offset: Number(response.headers.get("upload-offset") || String(offset + chunk.size)),
      finalUrl: response.headers.get("location"),
    };
  }

  tusForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const file = tusInput && tusInput.files ? tusInput.files[0] : null;
    if (!file) return;
    const expires = tusForm.querySelector("input[name=expires]")?.value.trim();
    if (tusButton) tusButton.disabled = true;
    if (tusProgress) {
      tusProgress.hidden = false;
      tusProgress.value = 0;
    }
    try {
      let location = storedLocation(file);
      if (!location) {
        location = await createTusUpload(file, expires);
        rememberLocation(file, location);
      }
      let offset = await currentTusOffset(location).catch(async () => {
        const fresh = await createTusUpload(file, expires);
        rememberLocation(file, fresh);
        location = fresh;
        return 0;
      });
      while (offset < file.size) {
        const result = await sendTusChunk(location, file, offset);
        offset = result.offset;
        if (tusProgress) tusProgress.value = Math.round((offset / file.size) * 100);
        if (result.finalUrl) {
          forgetLocation(file);
          setTusStatus("Upload complete: " + result.finalUrl, false);
        }
      }
      forgetLocation(file);
      if (tusProgress) tusProgress.value = 100;
      if (!tusStatus || tusStatus.hidden) setTusStatus("Upload complete", false);
    } catch (error) {
      setTusStatus(error.message || "Upload failed", true);
    } finally {
      if (tusButton) tusButton.disabled = false;
    }
  });
})();
