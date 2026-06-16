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

  document.body.addEventListener("htmx:afterSwap", (event) => {
    ensureCsrfFields(event.target);
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
  const chunkSize = 1024 * 1024;

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
      return;
    }
    const size = formatFileSize(file.size);
    uploadSelectedFile.textContent = size ? file.name + " (" + size + ")" : file.name;
    uploadSelectedFile.classList.remove("is-empty");
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

  async function createTusUpload(file, expires) {
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
    });
    if (!response.ok) throw new Error("Upload creation failed (" + response.status + ")");
    return new URL(response.headers.get("location"), window.location.origin).toString();
  }

  async function currentTusOffset(location) {
    const response = await fetch(location, {
      method: "HEAD",
      headers: { "Tus-Resumable": "1.0.0" },
    });
    if (!response.ok) throw new Error("Upload resume failed (" + response.status + ")");
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
    if (uploadButton) uploadButton.disabled = true;
    if (uploadProgress) {
      uploadProgress.hidden = false;
      uploadProgress.value = 0;
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
      setUploadStatus(error.message || "Upload failed", true);
    } finally {
      if (uploadButton) uploadButton.disabled = false;
    }
  });
})();
