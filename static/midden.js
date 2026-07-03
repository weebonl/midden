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
      if (!document.execCommand("copy")) {
        return Promise.reject(new Error("copy command failed"));
      }
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
      const original = copyButton.getAttribute("data-copy-label") || copyButton.textContent;
      copyButton.setAttribute("data-copy-label", original);
      window.middenCopy(value).then(() => {
        copyButton.setAttribute("data-copy-label", original);
        copyButton.textContent = "Copied";
        window.setTimeout(() => {
          copyButton.textContent = copyButton.getAttribute("data-copy-label") || original;
        }, 1600);
      }).catch(() => {
        copyButton.textContent = "Copy failed";
        window.setTimeout(() => {
          copyButton.textContent = copyButton.getAttribute("data-copy-label") || original;
        }, 2200);
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
      secretToggle.setAttribute("aria-pressed", showing ? "false" : "true");
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

  uploadForm.addEventListener("submit", (event) => {
    event.preventDefault();
    const file = uploadInput && uploadInput.files ? uploadInput.files[0] : null;
    if (!file) return;

    const formData = new FormData(uploadForm);
    uploadAbortController = new AbortController();
    setUploadStatus("Uploading...", false);

    if (uploadButton) uploadButton.disabled = true;
    if (uploadCancel) uploadCancel.hidden = false;
    if (uploadProgress) {
      uploadProgress.hidden = false;
      uploadProgress.value = 0;
    }

    const xhr = new XMLHttpRequest();
    xhr.open("POST", "/");
    xhr.setRequestHeader("Accept", "application/json");

    // Track upload progress
    xhr.upload.addEventListener("progress", (e) => {
      if (e.lengthComputable && uploadProgress) {
        uploadProgress.value = Math.round((e.loaded / e.total) * 100);
      }
    });

    xhr.addEventListener("load", () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        try {
          const result = JSON.parse(xhr.responseText);
          if (uploadProgress) uploadProgress.value = 100;
          setUploadCompleteStatus(result);
        } catch (err) {
          setUploadStatus("Failed to parse server response", true);
        }
      } else {
        setUploadStatus("Upload failed (" + xhr.status + ")", true);
      }
      cleanup();
    });

    xhr.addEventListener("error", () => {
      setUploadStatus("Upload failed", true);
      cleanup();
    });

    xhr.addEventListener("abort", () => {
      setUploadStatus("Upload canceled.", true);
      cleanup();
    });

    // Handle cancellation
    uploadAbortController.signal.addEventListener("abort", () => {
      xhr.abort();
    });

    function cleanup() {
      if (uploadButton) uploadButton.disabled = false;
      if (uploadCancel) uploadCancel.hidden = true;
      uploadAbortController = null;
      updateSelectedFile();
    }

    xhr.send(formData);
  });

  if (uploadCancel) {
    uploadCancel.addEventListener("click", () => {
      if (uploadAbortController) uploadAbortController.abort();
    });
  }
})();
