// First-run setup window. Talks to the shell through the global Tauri API
// (`withGlobalTauri`); no network access of its own.
(() => {
  "use strict";

  const invoke = window.__TAURI__.core.invoke;
  const el = (id) => document.getElementById(id);

  const form = el("setup-form");
  const pgFields = el("pg-fields");
  const radios = Array.from(document.querySelectorAll('input[name="mode"]'));
  const submit = el("submit");
  const statusBox = el("status");
  const logBox = el("log");
  const logText = el("log-text");

  const selectedMode = () => {
    const radio = radios.find((item) => item.checked);
    return radio ? radio.value : "sqlite";
  };

  const syncMode = () => {
    pgFields.hidden = selectedMode() !== "postgres";
  };

  const setBusy = (busy, label) => {
    submit.disabled = busy;
    submit.textContent = busy ? label || "启动中…" : "保存并启动";
  };

  const showStatus = (kind, title, detail) => {
    statusBox.hidden = false;
    statusBox.className = kind;
    const heading = document.createElement("strong");
    heading.textContent = title;
    statusBox.replaceChildren(heading);
    if (detail) {
      const body = document.createElement("div");
      body.textContent = detail;
      statusBox.appendChild(body);
    }
  };

  const showLog = (text) => {
    if (!text) {
      logBox.hidden = true;
      logText.textContent = "";
      return;
    }
    logBox.hidden = false;
    logText.textContent = text;
  };

  const describeError = (error) => {
    if (typeof error === "string") return error;
    if (error && typeof error.message === "string") return error.message;
    try {
      return JSON.stringify(error, null, 2);
    } catch {
      return String(error);
    }
  };

  const collectPayload = () => {
    if (selectedMode() === "sqlite") {
      return { mode: "sqlite" };
    }
    const port = Number.parseInt(el("pg-port").value, 10);
    return {
      mode: "postgres",
      postgres: {
        host: el("pg-host").value.trim(),
        port: Number.isFinite(port) ? port : 0,
        database: el("pg-database").value.trim(),
        user: el("pg-user").value.trim(),
        password: el("pg-password").value,
        sslmode: el("pg-sslmode").value,
      },
    };
  };

  const fill = (state) => {
    el("config-path").textContent = state.config_file || "";
    el("data-path").textContent = state.data_dir || "";
    el("sqlite-path").textContent = state.sqlite_path || "";
    const current = state.current;
    if (current) {
      for (const radio of radios) {
        radio.checked = radio.value === current.mode;
      }
      if (current.host) el("pg-host").value = current.host;
      if (current.port) el("pg-port").value = String(current.port);
      if (current.database) el("pg-database").value = current.database;
      if (current.user) el("pg-user").value = current.user;
      if (current.sslmode) el("pg-sslmode").value = current.sslmode;
      if (current.password_saved) {
        el("pg-password").placeholder = "已保存，留空沿用";
      }
    }
    if (state.last_error) {
      showStatus("error", "上一次启动失败", state.last_error);
      showLog(state.last_error);
    }
    syncMode();
  };

  for (const radio of radios) {
    radio.addEventListener("change", syncMode);
  }

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    setBusy(true, "启动中…");
    showLog("");
    try {
      const outcome = await invoke("save_and_launch", { payload: collectPayload() });
      if (outcome && outcome.warning) {
        showStatus("warning", "提示", outcome.warning);
        showLog(outcome.warning);
      } else {
        showStatus("success", "配置已保存，正在打开主窗口…");
      }
    } catch (error) {
      const message = describeError(error);
      showStatus("error", "启动失败", "请检查设置后重试；详细输出见下方日志。");
      showLog(message);
      setBusy(false);
    }
  });

  invoke("setup_state")
    .then((state) => {
      fill(state);
      setBusy(false);
    })
    .catch((error) => {
      showStatus("error", "无法读取设置", describeError(error));
      setBusy(false);
    });
})();
