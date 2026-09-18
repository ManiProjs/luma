"use strict";

/* ==========================================================================
   State
   ========================================================================== */

let socket = null;
let reconnectTimer = null;

let currentState = "Idle";
let currentFile = null;
let currentAssistantMessage = null;

let monacoEditor = null;
let currentFileContent = "";

let editorDirty = false;
let suppressEditorChange = false;

let ideOpen = false;

const openFiles = new Map();
const events = [];

/* ==========================================================================
   Elements
   ========================================================================== */

const elements = {
  explorer: document.getElementById("explorer"),
  workspaceName: document.getElementById("workspace-name"),

  connectionDot: document.getElementById("connection-dot"),

  connectionText: document.getElementById("connection-text"),

  currentFile: document.getElementById("current-file"),

  tabs: document.getElementById("tabs"),

  ide: document.getElementById("ide"),

  workspaceLayout: document.getElementById("workspace-layout"),

  toggleIde: document.getElementById("toggle-ide"),

  editor: document.getElementById("editor"),

  lineNumbers: document.getElementById("line-numbers"),

  editorPath: document.getElementById("editor-path"),

  editorLanguage: document.getElementById("editor-language"),

  editorPosition: document.getElementById("editor-position"),

  editorState: document.getElementById("editor-state"),

  saveFile: document.getElementById("save-file"),

  showDiff: document.getElementById("show-diff"),

  agentStatusDot: document.getElementById("agent-status-dot"),

  agentStatusDetail: document.getElementById("agent-status-detail"),

  modelName: document.getElementById("model-name"),

  messages: document.getElementById("messages"),

  chatEmpty: document.getElementById("chat-empty"),

  chatForm: document.getElementById("chat-form"),

  chatInput: document.getElementById("chat-input"),

  planPanel: document.getElementById("plan-panel"),

  plan: document.getElementById("plan"),

  planCount: document.getElementById("plan-count"),

  confirmation: document.getElementById("confirmation"),

  confirmationName: document.getElementById("confirmation-name"),

  confirmationInput: document.getElementById("confirmation-input"),

  allow: document.getElementById("allow"),

  deny: document.getElementById("deny"),

  terminalOutput: document.getElementById("terminal-output"),

  terminalForm: document.getElementById("terminal-form"),

  terminalCommand: document.getElementById("terminal-command"),

  clearTerminal: document.getElementById("clear-terminal"),

  newFile: document.getElementById("new-file"),

  newFolder: document.getElementById("new-folder"),

  refreshTree: document.getElementById("refresh-tree"),

  themeToggle: document.getElementById("theme-toggle"),

  toast: document.getElementById("toast"),
};

/* ==========================================================================
   WebSocket
   ========================================================================== */

function connect() {
  if (
    socket &&
    (socket.readyState === WebSocket.OPEN ||
      socket.readyState === WebSocket.CONNECTING)
  ) {
    return;
  }

  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";

  socket = new WebSocket(`${protocol}//${window.location.host}/ws`);

  socket.addEventListener("open", () => {
    setConnection(true);

    send({
      type: "GetTree",
    });
  });

  socket.addEventListener("message", (event) => {
    try {
      const message = JSON.parse(event.data);

      handleServerMessage(message);
    } catch (error) {
      console.error("Invalid dashboard message:", error);
    }
  });

  socket.addEventListener("close", () => {
    setConnection(false);

    scheduleReconnect();
  });

  socket.addEventListener("error", () => {
    setConnection(false);
  });
}

function scheduleReconnect() {
  if (reconnectTimer !== null) {
    return;
  }

  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;

    connect();
  }, 1500);
}

function send(message) {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    showToast("Dashboard is not connected.");

    return false;
  }

  socket.send(JSON.stringify(message));

  return true;
}

function setConnection(connected) {
  elements.connectionDot.className = `connection-dot ${
    connected ? "connected" : "disconnected"
  }`;

  elements.connectionText.textContent = connected
    ? "Connected"
    : "Disconnected";
}

/* ==========================================================================
   Server messages
   ========================================================================== */

function handleServerMessage(message) {
  if (message.type === "Event") {
    handleAgentEvent(message.data);

    return;
  }

  switch (message.type) {
    case "FileOpened":
      openFileInEditor(message.data.path, message.data.content);
      break;

    case "FileSaved":
      handleFileSaved(message.data.path);
      break;

    case "FileCreated":
      refreshTree();

      openFile(message.data.path);
      break;

    case "DirectoryCreated":
      refreshTree();
      break;

    case "Deleted":
      handleDeleted(message.data.path);
      break;

    case "Renamed":
      handleRenamed(message.data.from, message.data.to);
      break;

    case "DirectoryTree":
      renderExplorer(message.data.entries);
      break;

    case "Diff":
      showDiff(message.data.path, message.data.diff);
      break;

    case "TerminalOutput":
      appendTerminal(message.data.command, message.data.output);
      break;

    case "Error":
      showToast(message.data.message);

      addSystemMessage(message.data.message);

      break;

    default:
      console.warn("Unknown dashboard message:", message);
  }
}

/* ==========================================================================
   Agent
   ========================================================================== */

function handleAgentEvent(event) {
  events.push({
    event,
    time: new Date(),
  });

  switch (event.type) {
    case "Status":
      updateAgentStatus(event.data);
      break;

    case "Thinking":
      setAgentStatus("Thinking", "Reasoning about the task");
      break;

    case "PlanGenerated":
      setAgentStatus("Thinking", "Planning the implementation");

      renderPlan(event.data);

      break;

    case "ToolStarted":
      setAgentStatus("Doing", `Running ${event.data.name}`);

      addToolMessage(event.data);

      break;

    case "ToolFinished":
      setAgentStatus("Doing", `${event.data.name} finished`);

      addToolFinishedMessage(event.data);

      break;

    case "TextDelta":
      setAgentStatus("Talking", "Generating response");

      appendAssistantText(event.data);

      break;

    case "ConfirmationRequired":
      setAgentStatus("Waiting", "Waiting for your approval");

      showConfirmation(event.data);

      break;

    case "SystemMessage":
      addSystemMessage(event.data);

      break;

    case "Error":
      setAgentStatus("Error", "Something went wrong");

      addSystemMessage(event.data);

      break;

    case "Usage":
      updateUsage(event.data);

      break;

    case "Finished":
      setAgentStatus("Idle", "Ready for another task");

      currentAssistantMessage = null;

      break;
  }
}

/* ==========================================================================
   Agent status
   ========================================================================== */

function updateAgentStatus(data) {
  const raw = typeof data === "string" ? data : (data?.state ?? "idle");

  const state = String(raw).toLowerCase();

  const labels = {
    idle: ["Idle", "Ready for another task"],

    thinking: ["Thinking", "Reasoning about the task"],

    doing: ["Doing", "Working on the task"],

    talking: ["Talking", "Generating a response"],

    waiting: ["Waiting", "Waiting for your approval"],

    error: ["Error", "Something went wrong"],
  };

  const [label, detail] = labels[state] ?? [String(raw), "Agent status"];

  setAgentStatus(label, detail);
}

function setAgentStatus(label, detail) {
  currentState = label;

  elements.agentStatusDetail.textContent = detail;

  elements.agentStatusDot.className = "agent-dot";

  const state = label.toLowerCase();

  if (["thinking", "doing", "talking", "waiting"].includes(state)) {
    elements.agentStatusDot.classList.add("running");
  }

  if (state === "error") {
    elements.agentStatusDot.classList.add("error");
  }
}

/* ==========================================================================
   IDE
   ========================================================================== */

function setIdeOpen(open) {
  ideOpen = open;

  elements.ide.classList.toggle("hidden", !open);

  elements.workspaceLayout.classList.toggle("ide-open", open);

  elements.toggleIde.textContent = open ? "Close IDE" : "Open IDE";

  elements.toggleIde.classList.toggle("open", open);

  if (open && monacoEditor) {
    requestAnimationFrame(() => {
      monacoEditor.layout();
    });
  }
}

elements.toggleIde.addEventListener("click", () => {
  setIdeOpen(!ideOpen);
});

/* ==========================================================================
   Explorer
   ========================================================================== */

function renderExplorer(entries) {
  elements.explorer.innerHTML = "";

  if (!entries || entries.length === 0) {
    elements.explorer.innerHTML = `<div class="empty">
        Workspace is empty.
      </div>`;

    return;
  }

  const root = createTreeRoot();

  for (const entry of entries) {
    insertTreeEntry(root, entry);
  }

  elements.explorer.appendChild(root);

  elements.workspaceName.textContent = "Current Workspace";
}

function createTreeRoot() {
  const root = document.createElement("div");

  root.className = "tree-root";

  return root;
}

function insertTreeEntry(root, entry) {
  const parts = entry.path.split("/").filter(Boolean);

  let container = root;
  let currentPath = "";

  for (let i = 0; i < parts.length; i++) {
    const part = parts[i];

    currentPath = currentPath ? `${currentPath}/${part}` : part;

    let node = container.querySelector(
      `:scope > [data-path="${CSS.escape(currentPath)}"]`,
    );

    if (!node) {
      node = createTreeNode(
        part,
        currentPath,
        i === parts.length - 1 ? entry.kind : "directory",
      );

      container.appendChild(node);

      if (i < parts.length - 1 || entry.kind === "directory") {
        const children = document.createElement("div");

        children.className = "tree-children";

        node.appendChild(children);
      }
    }

    const children = node.querySelector(":scope > .tree-children");

    if (children) {
      container = children;
    }
  }
}

function createTreeNode(name, path, kind) {
  const node = document.createElement("div");

  node.className = `tree-node ${kind}`;

  node.dataset.path = path;

  const button = document.createElement("button");

  button.className = "tree-row";

  const icon = document.createElement("span");

  icon.className = "tree-icon";

  icon.textContent = kind === "directory" ? "▾" : fileIcon(name);

  const label = document.createElement("span");

  label.className = "tree-name";

  label.textContent = name;

  button.appendChild(icon);

  button.appendChild(label);

  button.addEventListener("click", (event) => {
    event.stopPropagation();

    if (kind === "directory") {
      node.classList.toggle("collapsed");

      icon.textContent = node.classList.contains("collapsed") ? "›" : "▾";

      return;
    }

    openFile(path);
  });

  node.appendChild(button);

  return node;
}

function fileIcon(name) {
  const lower = name.toLowerCase();

  if (lower.endsWith(".rs")) return "R";

  if (lower.endsWith(".js")) return "JS";

  if (lower.endsWith(".ts")) return "TS";

  if (lower.endsWith(".py")) return "PY";

  if (lower.endsWith(".json")) return "{}";

  if (lower.endsWith(".toml")) return "T";

  if (lower.endsWith(".md")) return "M";

  if (lower.endsWith(".css")) return "#";

  if (lower.endsWith(".html")) return "H";

  return "·";
}

function refreshTree() {
  send({
    type: "GetTree",
  });
}

/* ==========================================================================
   Editor
   ========================================================================== */

function openFile(path) {
  if (currentFile === path && openFiles.has(path)) {
    setIdeOpen(true);

    return;
  }

  send({
    type: "OpenFile",

    data: {
      path,
    },
  });
}

function openFileInEditor(path, content) {
  /*
   * Opening a file explicitly means:
   *
   * "I need the IDE now."
   */

  setIdeOpen(true);

  openFiles.set(path, {
    content,
    savedContent: content,
    dirty: false,
  });

  currentFile = path;
  currentFileContent = content;
  editorDirty = false;

  initializeMonaco(() => {
    suppressEditorChange = true;

    const language = languageFromPath(path);

    const model = monaco.editor.createModel(content, language);

    if (monacoEditor.getModel()) {
      monacoEditor.getModel().dispose();
    }

    monacoEditor.setModel(model);

    suppressEditorChange = false;

    elements.currentFile.textContent = path;

    elements.editorPath.textContent = path;

    elements.editorState.textContent = "Saved";

    elements.editorPosition.textContent = "Ln 1, Col 1";

    elements.editorLanguage.textContent = languageName(language);

    elements.saveFile.disabled = true;

    elements.showDiff.disabled = false;

    updateEditorUI();
    renderTabs();

    requestAnimationFrame(() => {
      monacoEditor.layout();
      monacoEditor.focus();
    });
  });
}

function languageFromPath(path) {
  const extension = path.split(".").pop()?.toLowerCase();

  const languages = {
    rs: "rust",

    js: "javascript",
    jsx: "javascript",
    mjs: "javascript",
    cjs: "javascript",

    ts: "typescript",
    tsx: "typescript",

    py: "python",

    go: "go",

    c: "c",
    h: "c",
    cpp: "cpp",
    cc: "cpp",
    hpp: "cpp",

    java: "java",

    cs: "csharp",

    json: "json",

    html: "html",
    htm: "html",

    css: "css",
    scss: "scss",

    xml: "xml",

    yaml: "yaml",
    yml: "yaml",

    toml: "ini",

    sh: "shell",
    bash: "shell",

    md: "markdown",

    sql: "sql",
  };

  return languages[extension] ?? "plaintext";
}

function languageName(language) {
  const names = {
    rust: "Rust",
    javascript: "JavaScript",
    typescript: "TypeScript",
    python: "Python",
    go: "Go",
    c: "C",
    cpp: "C++",
    java: "Java",
    csharp: "C#",
    json: "JSON",
    html: "HTML",
    css: "CSS",
    scss: "SCSS",
    xml: "XML",
    yaml: "YAML",
    ini: "TOML",
    shell: "Shell",
    markdown: "Markdown",
    sql: "SQL",
    plaintext: "Plain Text",
  };

  return names[language] ?? "Plain Text";
}

function updateEditorUI() {
  if (!currentFile) {
    elements.currentFile.textContent = ideOpen ? "No file open" : "Agent";

    elements.editorPath.textContent = "No file open";

    elements.editorLanguage.textContent = "Plain Text";

    elements.editorState.textContent = "No file";

    elements.saveFile.disabled = true;

    elements.showDiff.disabled = true;

    return;
  }

  elements.currentFile.textContent = `${currentFile}${editorDirty ? " •" : ""}`;

  elements.editorPath.textContent = currentFile;

  const language = languageFromPath(currentFile);

  elements.editorLanguage.textContent = languageName(language);

  elements.editorState.textContent = editorDirty ? "Modified" : "Saved";

  elements.saveFile.disabled = !editorDirty;

  elements.showDiff.disabled = false;
}

function renderTabs() {
  elements.tabs.innerHTML = "";

  for (const [path, file] of openFiles) {
    const tab = document.createElement("div");

    tab.className = "tab";

    if (path === currentFile) {
      tab.classList.add("active");
    }

    const label = document.createElement("span");

    label.textContent = basename(path);

    const dirty = document.createElement("span");

    dirty.className = "tab-dirty";

    dirty.textContent = file.dirty ? "●" : "";

    const close = document.createElement("button");

    close.className = "tab-close";

    close.textContent = "×";

    close.addEventListener("click", (event) => {
      event.stopPropagation();

      closeFile(path);
    });

    tab.appendChild(label);

    tab.appendChild(dirty);

    tab.appendChild(close);

    tab.addEventListener("click", () => {
      switchToFile(path);
    });

    elements.tabs.appendChild(tab);
  }
}

function switchToFile(path) {
  const file = openFiles.get(path);

  if (!file) {
    openFile(path);

    return;
  }

  setIdeOpen(true);

  currentFile = path;

  currentFileContent = file.savedContent;

  editorDirty = file.dirty;

  initializeMonaco(() => {
    suppressEditorChange = true;

    const language = languageFromPath(path);

    const model = monaco.editor.createModel(file.content, language);

    if (monacoEditor.getModel()) {
      monacoEditor.getModel().dispose();
    }

    monacoEditor.setModel(model);

    suppressEditorChange = false;

    updateEditorUI();
    renderTabs();

    requestAnimationFrame(() => {
      monacoEditor.layout();
      monacoEditor.focus();
    });
  });
}

function closeFile(path) {
  openFiles.delete(path);

  if (currentFile === path) {
    const next = openFiles.keys().next().value;

    if (next) {
      switchToFile(next);

      return;
    }

    currentFile = null;
    currentFileContent = "";
    editorDirty = false;

    if (monacoEditor) {
      suppressEditorChange = true;

      const model = monaco.editor.createModel("", "plaintext");

      if (monacoEditor.getModel()) {
        monacoEditor.getModel().dispose();
      }

      monacoEditor.setModel(model);

      suppressEditorChange = false;
    }

    updateEditorUI();
    renderTabs();

    return;
  }

  renderTabs();
}

function initializeMonaco(callback = null) {
  if (monacoEditor) {
    if (callback) {
      callback();
    }

    return;
  }

  require(["vs/editor/editor.main"], () => {
    monacoEditor = monaco.editor.create(elements.editor, {
      value: "",
      language: "plaintext",

      theme:
        document.documentElement.getAttribute("data-theme") === "dark"
          ? "vs-dark"
          : "vs",

      automaticLayout: true,

      minimap: {
        enabled: false,
      },

      fontSize: 13,

      lineHeight: 21,

      padding: {
        top: 12,
        bottom: 12,
      },

      scrollBeyondLastLine: false,

      smoothScrolling: true,

      cursorBlinking: "smooth",

      renderWhitespace: "selection",

      tabSize: 2,

      insertSpaces: true,

      wordWrap: "off",

      suggest: {
        showMethods: true,
        showFunctions: true,
        showClasses: true,
        showVariables: true,
      },
    });

    monacoEditor.onDidChangeCursorPosition((event) => {
      elements.editorPosition.textContent = `Ln ${event.position.lineNumber}, Col ${event.position.column}`;
    });

    monacoEditor.onDidChangeModelContent(() => {
      if (suppressEditorChange || !currentFile || !monacoEditor.getModel()) {
        return;
      }

      const file = openFiles.get(currentFile);

      if (!file) {
        return;
      }

      file.content = monacoEditor.getValue();

      file.dirty = file.content !== file.savedContent;

      editorDirty = file.dirty;

      elements.editorState.textContent = file.dirty ? "Modified" : "Saved";

      elements.saveFile.disabled = !file.dirty;

      elements.currentFile.textContent = `${currentFile}${
        file.dirty ? " •" : ""
      }`;

      renderTabs();
    });

    if (callback) {
      callback();
    }
  });
}

/* ==========================================================================
   Save
   ========================================================================== */

function saveCurrentFile() {
  if (!currentFile || !editorDirty || !monacoEditor) {
    return;
  }

  send({
    type: "SaveFile",

    data: {
      path: currentFile,
      content: monacoEditor.getValue(),
    },
  });
}

function handleFileSaved(path) {
  const file = openFiles.get(path);

  if (!file) {
    return;
  }

  const content =
    path === currentFile && monacoEditor
      ? monacoEditor.getValue()
      : file.content;

  file.content = content;

  file.savedContent = content;

  file.dirty = false;

  if (currentFile === path) {
    currentFileContent = content;

    editorDirty = false;

    elements.editorState.textContent = "Saved";

    elements.saveFile.disabled = true;
  }

  renderTabs();

  showToast(`Saved ${path}`);
}

/* ==========================================================================
   Diff
   ========================================================================== */

function requestDiff() {
  if (!currentFile) {
    return;
  }

  send({
    type: "GetDiff",

    data: {
      path: currentFile,
    },
  });
}

function showDiff(path, diff) {
  const content = diff.trim() ? diff : "No changes.";

  addMessage("system", `${path}\n\n${content}`);
}

/* ==========================================================================
   Agent chat
   ========================================================================== */

elements.chatForm.addEventListener("submit", (event) => {
  event.preventDefault();

  sendChat();
});

function sendChat(text = null) {
  const value = (text ?? elements.chatInput.value).trim();

  if (!value) {
    return;
  }

  if (
    !send({
      type: "Chat",
      data: value,
    })
  ) {
    return;
  }

  addMessage("user", value);

  elements.chatInput.value = "";

  autoResizeChat();

  currentAssistantMessage = null;

  elements.chatInput.focus();

  setAgentStatus("Thinking", "Reasoning about the task");
}

function addMessage(role, text) {
  elements.chatEmpty.style.display = "none";

  const message = document.createElement("div");

  message.className = `message ${role}`;

  const header = document.createElement("div");

  header.className = "message-header";

  header.textContent = role === "user" ? "You" : "Luma";

  const content = document.createElement("div");

  content.className = "message-content";

  content.textContent = text;

  message.appendChild(header);

  message.appendChild(content);

  elements.messages.appendChild(message);

  elements.messages.scrollTop = elements.messages.scrollHeight;

  return content;
}

function appendAssistantText(text) {
  if (!currentAssistantMessage) {
    currentAssistantMessage = addMessage("assistant", "");
  }

  currentAssistantMessage.textContent += text;

  elements.messages.scrollTop = elements.messages.scrollHeight;
}

function addSystemMessage(text) {
  addMessage("system", text);
}

function addToolMessage(tool) {
  const message = addMessage(
    "system",
    `Running ${tool.name}\n${tool.input ?? ""}`,
  );

  message.parentElement.classList.add("tool-message");
}

function addToolFinishedMessage(tool) {
  addMessage("system", `${tool.name} finished in ${tool.duration_ms} ms`);
}

/* ==========================================================================
   Plan
   ========================================================================== */

function renderPlan(plan) {
  const lines = String(plan)
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

  elements.planPanel.classList.remove("hidden");

  elements.plan.innerHTML = "";

  elements.planCount.textContent = `${lines.length} steps`;

  for (const line of lines) {
    const item = document.createElement("div");

    item.className = "plan-item";

    const indicator = document.createElement("span");

    indicator.className = "plan-indicator";

    const title = document.createElement("span");

    title.className = "plan-title";

    title.textContent = line.replace(/^[-*]\s*/, "");

    item.appendChild(indicator);

    item.appendChild(title);

    elements.plan.appendChild(item);
  }
}

/* ==========================================================================
   Confirmation
   ========================================================================== */

function showConfirmation(data) {
  elements.confirmationName.textContent = data.name;

  elements.confirmationInput.textContent = data.input;

  elements.confirmation.classList.remove("hidden");
}

function hideConfirmation() {
  elements.confirmation.classList.add("hidden");
}

elements.allow.addEventListener("click", () => {
  send({
    type: "Confirm",

    data: {
      allowed: true,
    },
  });

  hideConfirmation();

  setAgentStatus("Doing", "Action approved");
});

elements.deny.addEventListener("click", () => {
  send({
    type: "Confirm",

    data: {
      allowed: false,
    },
  });

  hideConfirmation();

  setAgentStatus("Thinking", "Rethinking the task");
});

/* ==========================================================================
   Terminal
   ========================================================================== */

elements.terminalForm.addEventListener("submit", (event) => {
  event.preventDefault();

  const command = elements.terminalCommand.value.trim();

  if (!command) {
    return;
  }

  send({
    type: "Terminal",

    data: {
      command,
    },
  });

  appendTerminal("$ " + command, "");

  elements.terminalCommand.value = "";
});

function appendTerminal(command, output) {
  const block = document.createElement("div");

  block.className = "terminal-block";

  const commandLine = document.createElement("div");

  commandLine.className = "terminal-command";

  commandLine.textContent = command.startsWith("$ ") ? command : `$ ${command}`;

  block.appendChild(commandLine);

  if (output) {
    const result = document.createElement("pre");

    result.textContent = output;

    block.appendChild(result);
  }

  elements.terminalOutput.appendChild(block);

  elements.terminalOutput.scrollTop = elements.terminalOutput.scrollHeight;
}

elements.clearTerminal.addEventListener("click", () => {
  elements.terminalOutput.innerHTML = "";
});

/* ==========================================================================
   File creation
   ========================================================================== */

elements.newFile.addEventListener("click", () => {
  const path = window.prompt("New file path:", "src/new_file.rs");

  if (!path) {
    return;
  }

  send({
    type: "CreateFile",

    data: {
      path,
      content: "",
    },
  });
});

elements.newFolder.addEventListener("click", () => {
  const path = window.prompt("New folder path:", "src/new_folder");

  if (!path) {
    return;
  }

  send({
    type: "CreateDirectory",

    data: {
      path,
    },
  });
});

function handleDeleted(path) {
  const pathsToClose = [];

  for (const openPath of openFiles.keys()) {
    if (openPath === path || openPath.startsWith(`${path}/`)) {
      pathsToClose.push(openPath);
    }
  }

  for (const openPath of pathsToClose) {
    closeFile(openPath);
  }

  refreshTree();

  showToast(`Deleted ${path}`);
}

function handleRenamed(from, to) {
  const file = openFiles.get(from);

  if (file) {
    openFiles.delete(from);

    openFiles.set(to, file);
  }

  if (currentFile === from) {
    currentFile = to;
  }

  renderTabs();

  updateEditorUI();

  refreshTree();

  showToast(`Renamed ${from} → ${to}`);
}

/* ==========================================================================
   Suggestions
   ========================================================================== */

document.querySelectorAll("[data-prompt]").forEach((button) => {
  button.addEventListener("click", () => {
    sendChat(button.dataset.prompt);
  });
});

/* ==========================================================================
   Keyboard shortcuts
   ========================================================================== */

document.addEventListener("keydown", (event) => {
  const modifier = event.metaKey || event.ctrlKey;

  if (modifier && event.key.toLowerCase() === "s") {
    event.preventDefault();

    if (!elements.saveFile.disabled) {
      saveCurrentFile();
    }
  }

  if (event.metaKey && event.key.toLowerCase() === "p") {
    event.preventDefault();

    elements.chatInput.focus();
  }

  /*
   * Cmd/Ctrl + J
   *
   * Toggle the optional IDE.
   */

  if (modifier && event.key.toLowerCase() === "j") {
    event.preventDefault();

    setIdeOpen(!ideOpen);
  }
});

elements.chatInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();

    sendChat();
  }
});

elements.chatInput.addEventListener("input", autoResizeChat);

function autoResizeChat() {
  elements.chatInput.style.height = "auto";

  elements.chatInput.style.height = `${Math.min(
    elements.chatInput.scrollHeight,
    160,
  )}px`;
}

/* ==========================================================================
   UI
   ========================================================================== */

elements.saveFile.addEventListener("click", saveCurrentFile);

elements.showDiff.addEventListener("click", requestDiff);

elements.refreshTree.addEventListener("click", refreshTree);

function basename(path) {
  return path.split("/").pop();
}

function showToast(message) {
  elements.toast.textContent = message;

  elements.toast.classList.remove("hidden");

  clearTimeout(showToast.timer);

  showToast.timer = setTimeout(() => {
    elements.toast.classList.add("hidden");
  }, 2200);
}

/* ==========================================================================
   Usage
   ========================================================================== */

function updateUsage(data) {
  /*
   * Kept intentionally lightweight.
   *
   * Usage data is available to the
   * dashboard without adding another
   * visible panel.
   */

  if (!data) {
    return;
  }

  if (data.model && typeof data.model === "string") {
    elements.modelName.textContent = data.model;
  }
}

/* ==========================================================================
   Theme
   ========================================================================== */

function loadTheme() {
  const theme = localStorage.getItem("luma-theme");

  if (theme === "dark") {
    document.documentElement.setAttribute("data-theme", "dark");

    elements.themeToggle.textContent = "☀";
  }
}

elements.themeToggle.addEventListener("click", () => {
  const dark = document.documentElement.getAttribute("data-theme") === "dark";

  if (dark) {
    document.documentElement.removeAttribute("data-theme");

    localStorage.setItem("luma-theme", "light");

    elements.themeToggle.textContent = "☾";

    if (monacoEditor) {
      monaco.editor.setTheme("vs");
    }
  } else {
    document.documentElement.setAttribute("data-theme", "dark");

    localStorage.setItem("luma-theme", "dark");

    elements.themeToggle.textContent = "☀";

    if (monacoEditor) {
      monaco.editor.setTheme("vs-dark");
    }
  }
});

/* ==========================================================================
   Initialization
   ========================================================================== */

loadTheme();

/*
 * IMPORTANT:
 *
 * We intentionally do NOT call
 * setIdeOpen(true) here.
 *
 * The dashboard starts agent-first.
 */

setIdeOpen(false);

initializeMonaco();

updateEditorUI();

connect();
