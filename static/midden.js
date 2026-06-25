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

  function ensureCsrfFields(root) {
    root.querySelectorAll("form").forEach((form) => {
      if ((form.method || "").toLowerCase() === "post") {
        ensureCsrfField(form);
        form.addEventListener("submit", () => ensureCsrfField(form));
      }
    });
  }

  ensureCsrfFields(document);

  function closestElement(value, selector) {
    if (!value || !value.closest) return null;
    return value.closest(selector);
  }

  function setRequestBusy(elt, busy) {
    const element = elt && elt.nodeType === 1 ? elt : null;
    const container = closestElement(element, "form") || element;
    if (!container) return;
    if (busy) {
      container.setAttribute("aria-busy", "true");
    } else {
      container.removeAttribute("aria-busy");
    }
  }

  document.body.addEventListener("htmx:beforeRequest", (event) => {
    setRequestBusy(event.detail && event.detail.elt, true);
    const globalError = document.getElementById("global-error");
    if (globalError) {
      globalError.replaceChildren();
    }
  });

  document.body.addEventListener("htmx:afterRequest", (event) => {
    setRequestBusy(event.detail && event.detail.elt, false);
  });

  document.body.addEventListener("htmx:beforeOnLoad", (event) => {
    const xhr = event.detail.xhr;
    if (xhr.status >= 400) {
      event.detail.shouldSwap = true;
      event.detail.target = document.getElementById("global-error") || event.detail.target;
    }
  });

  document.body.addEventListener("htmx:afterSwap", (event) => {
    ensureCsrfFields(event.target);
    if (window.Alpine && event.target) window.Alpine.initTree(event.target);
    if (!event.target || !event.target.querySelector) return;
    const heading = event.target.querySelector("[data-results-heading], h2, h3");
    if (!heading) return;
    if (!heading.hasAttribute("tabindex")) heading.setAttribute("tabindex", "-1");
    heading.focus({ preventScroll: true });
  });

  document.body.addEventListener("htmx:configRequest", (event) => {
    const token = readCookie(csrfCookie);
    if (!token) return;
    event.detail.headers["X-CSRF-Token"] = decodeURIComponent(token);
  });

  window.middenCopy = function (text) {
    if (navigator.clipboard && window.isSecureContext) {
      return navigator.clipboard.writeText(text);
    }
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.setAttribute("readonly", "");
    textarea.style.position = "fixed";
    textarea.style.top = "-1000px";
    document.body.appendChild(textarea);
    textarea.select();
    try {
      document.execCommand("copy");
      return Promise.resolve();
    } finally {
      textarea.remove();
    }
  };

  document.body.addEventListener("click", (event) => {
    const copyButton = closestElement(event.target, "[data-copy-value], [data-copy-source]");
    if (copyButton) {
      const source = copyButton.getAttribute("data-copy-source");
      const sourceElement = source ? document.querySelector(source) : null;
      const value = sourceElement ? sourceElement.value || sourceElement.textContent : copyButton.getAttribute("data-copy-value");
      if (!value) return;
      window.middenCopy(value).then(() => {
        const original = copyButton.getAttribute("data-copy-label") || copyButton.textContent;
        copyButton.setAttribute("data-copy-label", original);
        copyButton.textContent = "Copied";
        window.setTimeout(() => {
          copyButton.textContent = copyButton.getAttribute("data-copy-label") || original;
        }, 1600);
      });
    }

    const secretToggle = closestElement(event.target, "[data-secret-toggle]");
    if (secretToggle) {
      const field = closestElement(secretToggle, ".secret-field");
      const input = field && field.querySelector("[data-secret-input]");
      if (!input) return;
      const showing = input.type === "text";
      input.type = showing ? "password" : "text";
      secretToggle.textContent = showing ? "Show" : "Hide";
    }
  });

  document.querySelectorAll("[data-settings-section]").forEach((section) => {
    const name = section.getAttribute("data-settings-section");
    const key = "midden:settings-section:" + name;
    try {
      const stored = window.localStorage.getItem(key);
      if (stored === "open") section.open = true;
      if (stored === "closed") section.open = false;
    } catch (_) {}
    section.addEventListener("toggle", () => {
      try {
        window.localStorage.setItem(key, section.open ? "open" : "closed");
      } catch (_) {}
    });
  });

  document.querySelectorAll("[data-accent-input]").forEach((input) => {
    const preview = input.closest(".accent-field")?.querySelector("[data-accent-preview]");
    if (!preview) return;
    const update = () => {
      preview.style.background = input.value;
    };
    input.addEventListener("input", update);
    update();
  });

  function setupDropZone(dropZone, input, onFilesChanged) {
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
        onFilesChanged();
      }
    });
  }

  const uploadForm = document.querySelector("[data-browser-upload-form]");
  if (!uploadForm) return;

  const uploadInput = uploadForm.querySelector("input[type=file]");
  const uploadDropZone = uploadForm.querySelector("[data-drop-zone]");
  const uploadProgress = uploadForm.querySelector("[data-upload-progress]");
  const uploadStatus = uploadForm.querySelector("[data-upload-status]");
  const uploadSelectedFile = uploadForm.querySelector("[data-selected-file]");
  const uploadButton = uploadForm.querySelector("button[type=submit]");
  const uploadCancel = uploadForm.querySelector("[data-upload-cancel]");
  const uploadResume = uploadForm.querySelector("[data-upload-resume]");
  const configuredChunkSize = Number(uploadForm.getAttribute("data-upload-chunk-bytes") || "");
  const chunkSize = Number.isFinite(configuredChunkSize) && configuredChunkSize > 0 ? configuredChunkSize : 1024 * 1024;
  let uploadAbortController = null;

  function formatFileSize(bytes) {
    if (!Number.isFinite(bytes)) return "";
    if (bytes < 1024) return bytes + " B";
    const units = ["KB", "MB", "GB"];
    let size = bytes / 1024;
    let unit = units[0];
    for (let index = 1; index < units.length && size >= 1024; index += 1) {
      size /= 1024;
      unit = units[index];
    }
    return size.toFixed(size < 10 ? 1 : 0) + " " + unit;
  }

  function updateSelectedFile() {
    if (!uploadSelectedFile) return;
    const file = uploadInput && uploadInput.files ? uploadInput.files[0] : null;
    if (!file) {
      uploadSelectedFile.textContent = "No file selected";
      uploadSelectedFile.classList.add("is-empty");
      if (uploadResume) uploadResume.hidden = true;
      return;
    }
    const size = formatFileSize(file.size);
    uploadSelectedFile.textContent = size ? file.name + " (" + size + ")" : file.name;
    uploadSelectedFile.classList.remove("is-empty");
    if (uploadResume) {
      const location = storedLocation(file);
      uploadResume.hidden = !location;
      uploadResume.textContent = location ? "A previous upload session is available and will resume when you upload." : "";
    }
  }

  if (uploadDropZone && uploadInput) setupDropZone(uploadDropZone, uploadInput, updateSelectedFile);
  if (uploadInput) {
    uploadInput.addEventListener("change", updateSelectedFile);
    updateSelectedFile();
  }
  if (!window.fetch || !window.Blob) return;

  function setUploadStatus(message, isError) {
    if (!uploadStatus) return;
    uploadStatus.hidden = false;
    uploadStatus.textContent = message;
    uploadStatus.classList.toggle("error", Boolean(isError));
  }

  function absoluteUrl(value) {
    if (!value) return null;
    return new URL(value, window.location.origin).toString();
  }

  function appendLink(parent, label, href) {
    const link = document.createElement("a");
    link.href = href;
    link.textContent = label;
    parent.appendChild(link);
  }

  function setUploadCompleteStatus(result) {
    if (!uploadStatus) return;
    uploadStatus.hidden = false;
    uploadStatus.classList.remove("error");
    uploadStatus.replaceChildren();

    const headline = document.createElement("p");
    headline.appendChild(document.createTextNode("Upload complete: "));
    if (result.finalUrl) {
      appendLink(headline, result.finalUrl, result.finalUrl);
    } else {
      headline.appendChild(document.createTextNode("file saved"));
    }
    uploadStatus.appendChild(headline);

    if (result.rawUrl || result.deleteUrl) {
      const links = document.createElement("p");
      if (result.rawUrl) appendLink(links, "Raw file", result.rawUrl);
      if (result.rawUrl && result.deleteUrl) {
        links.appendChild(document.createTextNode(" | "));
      }
      if (result.deleteUrl) appendLink(links, "Delete", result.deleteUrl);
      uploadStatus.appendChild(links);
    }

    if (result.deleteToken) {
      const token = document.createElement("p");
      token.appendChild(document.createTextNode("Delete token, shown once: "));
      const code = document.createElement("code");
      code.textContent = result.deleteToken;
      token.appendChild(code);
      uploadStatus.appendChild(token);
    }
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

  async function createTusUpload(file, expires, signal) {
    const metadata = [
      "filename " + metadataValue(file.name || "upload.bin"),
      "content-type " + metadataValue(file.type || "application/octet-stream"),
    ];
    if (expires) metadata.push("expires " + metadataValue(expires));
    const visibility = uploadForm.querySelector("select[name=visibility]")?.value;
    if (visibility) metadata.push("visibility " + metadataValue(visibility));
    const headers = {
      "Tus-Resumable": "1.0.0",
      "Upload-Length": String(file.size),
      "Upload-Metadata": metadata.join(","),
    };
    const csrf = readCookie(csrfCookie);
    if (csrf) headers["X-CSRF-Token"] = decodeURIComponent(csrf);
    const response = await fetch("/tus", {
      method: "POST",
      headers,
      signal,
    });
    if (!response.ok) throw new Error("Upload creation failed (" + response.status + ")");
    return new URL(response.headers.get("location"), window.location.origin).toString();
  }

  async function currentTusOffset(location, signal) {
    const response = await fetch(location, {
      method: "HEAD",
      headers: { "Tus-Resumable": "1.0.0" },
      signal,
    });
    if (!response.ok) throw new Error("Upload resume failed (" + response.status + ")");
    return Number(response.headers.get("upload-offset") || "0");
  }

  async function sendTusChunk(location, file, offset, signal) {
    const chunk = file.slice(offset, Math.min(file.size, offset + chunkSize));
    const response = await fetch(location, {
      method: "PATCH",
      headers: {
        "Tus-Resumable": "1.0.0",
        "Upload-Offset": String(offset),
        "Content-Type": "application/offset+octet-stream",
      },
      body: chunk,
      signal,
    });
    if (!response.ok) throw new Error("Upload chunk failed (" + response.status + ")");
    return {
      offset: Number(response.headers.get("upload-offset") || String(offset + chunk.size)),
      finalUrl: absoluteUrl(response.headers.get("location")),
      rawUrl: absoluteUrl(response.headers.get("x-midden-raw-url")),
      deleteUrl: absoluteUrl(response.headers.get("x-midden-delete-url")),
      deleteToken: response.headers.get("x-midden-delete-token"),
    };
  }

  uploadForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const file = uploadInput && uploadInput.files ? uploadInput.files[0] : null;
    if (!file) return;
    const expires = uploadForm.querySelector("input[name=expires]")?.value.trim();
    uploadAbortController = new AbortController();
    if (uploadButton) uploadButton.disabled = true;
    if (uploadCancel) uploadCancel.hidden = false;
    if (uploadProgress) {
      uploadProgress.hidden = false;
      uploadProgress.value = 0;
    }
    try {
      let location = storedLocation(file);
      if (!location) {
        location = await createTusUpload(file, expires, uploadAbortController.signal);
        rememberLocation(file, location);
      }
      let offset = await currentTusOffset(location, uploadAbortController.signal).catch(async () => {
        const fresh = await createTusUpload(file, expires, uploadAbortController.signal);
        rememberLocation(file, fresh);
        location = fresh;
        return 0;
      });
      while (offset < file.size) {
        const result = await sendTusChunk(location, file, offset, uploadAbortController.signal);
        offset = result.offset;
        if (uploadProgress) uploadProgress.value = Math.round((offset / file.size) * 100);
        if (result.finalUrl) {
          forgetLocation(file);
          setUploadCompleteStatus(result);
        }
      }
      forgetLocation(file);
      if (uploadProgress) uploadProgress.value = 100;
      if (!uploadStatus || uploadStatus.hidden) setUploadStatus("Upload complete", false);
    } catch (error) {
      if (error && error.name === "AbortError") {
        setUploadStatus("Upload canceled. Start again to resume from this browser.", true);
      } else {
        setUploadStatus(error.message || "Upload failed", true);
      }
    } finally {
      if (uploadButton) uploadButton.disabled = false;
      if (uploadCancel) uploadCancel.hidden = true;
      uploadAbortController = null;
      updateSelectedFile();
    }
  });

  if (uploadCancel) {
    uploadCancel.addEventListener("click", () => {
      if (uploadAbortController) uploadAbortController.abort();
    });
  }
})();
