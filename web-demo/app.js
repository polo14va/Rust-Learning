const $ = (id) => document.getElementById(id);

const storage = {
  get: (key, fallback = "") => localStorage.getItem(key) || fallback,
  set: (key, value) => localStorage.setItem(key, value || ""),
  clear: (keys) => keys.forEach((key) => localStorage.removeItem(key)),
};

const tokenKeys = ["access_token", "refresh_token", "id_token", "pkce_verifier", "oauth_client_id", "oauth_client_secret", "oauth_scope", "oauth_redirect", "last_code"]; 

function getBaseUrl() {
  let base = $("baseUrl")?.value?.trim() || "http://localhost:3000";
  if (!/^https?:\/\//i.test(base)) {
    base = `http://${base}`;
  }
  return base.replace(/\/$/, "");
}

function logMessage(message) {
  const consoleEl = $("console");
  if (!consoleEl) return;
  const timestamp = new Date().toLocaleTimeString();
  const formatted = `[${timestamp}] ${message}\n`;
  consoleEl.textContent = formatted + consoleEl.textContent;
}

function logJson(label, payload) {
  const pretty = JSON.stringify(payload, null, 2);
  logMessage(`${label}:\n${pretty}`);
}

async function requestJson(path, options = {}) {
  const url = `${getBaseUrl()}${path}`;
  const response = await fetch(url, {
    headers: { "Content-Type": "application/json", ...(options.headers || {}) },
    credentials: "include",
    ...options,
  });
  const text = await response.text();
  const isJson = text.startsWith("{") || text.startsWith("[");
  const data = isJson ? JSON.parse(text) : text;
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${JSON.stringify(data)}`);
  }
  return data;
}

async function requestForm(path, body, options = {}) {
  const url = `${getBaseUrl()}${path}`;
  const response = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
      ...(options.headers || {}),
    },
    body: new URLSearchParams(body),
    credentials: "include",
    ...options,
  });
  const text = await response.text();
  const isJson = text.startsWith("{") || text.startsWith("[");
  const data = isJson ? JSON.parse(text) : text;
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${JSON.stringify(data)}`);
  }
  return data;
}

function updateTokenFields() {
  if (!$("accessToken")) return;
  $("accessToken").value = storage.get("access_token");
  $("refreshToken").value = storage.get("refresh_token");
  $("idToken").value = storage.get("id_token");
}

function saveTokenFields() {
  storage.set("access_token", $("accessToken").value.trim());
  storage.set("refresh_token", $("refreshToken").value.trim());
  storage.set("id_token", $("idToken").value.trim());
}

async function generatePkce() {
  const verifier = Array.from(crypto.getRandomValues(new Uint8Array(32)))
    .map((b) => (b % 36).toString(36))
    .join("");
  const encoder = new TextEncoder();
  const data = encoder.encode(verifier);
  const digest = await crypto.subtle.digest("SHA-256", data);
  const base64 = btoa(String.fromCharCode(...new Uint8Array(digest)))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
  return { verifier, challenge: base64 };
}

function bindButton(id, handler) {
  const btn = $(id);
  if (btn) {
    btn.addEventListener("click", handler);
  }
}

async function handleLogin() {
  saveTokenFields();
  const payload = {
    username: $("username").value.trim(),
    password: $("password").value.trim(),
  };
  const data = await requestJson("/login", {
    method: "POST",
    body: JSON.stringify(payload),
  });
  storage.set("access_token", data.access_token);
  storage.set("refresh_token", data.refresh_token);
  updateTokenFields();
  logJson("Login", data);
}

function openLoginModal() {
  const modal = $("loginModal");
  const frame = $("loginFrame");
  const baseUrl = getBaseUrl();
  const errorBox = $("loginError");
  if (errorBox) {
    errorBox.textContent = "";
    errorBox.classList.add("hidden");
  }
  frame.src = `${baseUrl}/login?mode=token`;
  modal.classList.remove("hidden");
  modal.setAttribute("aria-hidden", "false");
}

function closeLoginModal() {
  const modal = $("loginModal");
  const frame = $("loginFrame");
  frame.src = "about:blank";
  modal.classList.add("hidden");
  modal.setAttribute("aria-hidden", "true");
}

async function handleRegister() {
  const payload = {
    username: $("username").value.trim(),
    password: $("password").value.trim(),
  };
  const data = await requestJson("/register", {
    method: "POST",
    body: JSON.stringify(payload),
  });
  storage.set("access_token", data.access_token);
  storage.set("refresh_token", data.refresh_token);
  updateTokenFields();
  logJson("Register", data);
}

async function handleRefresh() {
  saveTokenFields();
  const payload = {
    refresh_token: storage.get("refresh_token"),
  };
  const data = await requestJson("/refresh", {
    method: "POST",
    body: JSON.stringify(payload),
  });
  storage.set("access_token", data.access_token);
  updateTokenFields();
  logJson("Refresh", data);
}

async function handleLogout() {
  saveTokenFields();
  const payload = { refresh_token: storage.get("refresh_token") };
  const data = await requestJson("/logout", {
    method: "POST",
    body: JSON.stringify(payload),
  });
  storage.clear(["access_token", "refresh_token", "id_token"]);
  updateTokenFields();
  logJson("Logout", data);
  window.location.href = "index.html";
}


async function handleHealth() {
  const data = await requestJson("/health", { method: "GET" });
  logJson("Health", data);
}

async function handleUsers() {
  const data = await requestJson("/users", { method: "GET" });
  logJson("Users", data);
}

async function handleMetrics() {
  const url = `${getBaseUrl()}/metrics`;
  const response = await fetch(url);
  const text = await response.text();
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${text}`);
  }
  logMessage(`Metrics:\n${text.slice(0, 1200)}${text.length > 1200 ? "..." : ""}`);
}

async function handleRoot() {
  const data = await requestJson("/", { method: "GET" });
  logMessage(`Root: ${data}`);
}

async function handleOpenId() {
  const data = await requestJson("/.well-known/openid-configuration", { method: "GET" });
  logJson("OpenID config", data);
}

async function handleJwks() {
  const data = await requestJson("/.well-known/jwks.json", { method: "GET" });
  logJson("JWKS", data);
}

async function handleDashboard() {
  saveTokenFields();
  const token = storage.get("access_token");
  const data = await requestJson("/dashboard", {
    method: "GET",
    headers: { Authorization: `Bearer ${token}` },
  });
  logJson("Dashboard", data);
}

async function handleUserinfo() {
  saveTokenFields();
  const token = storage.get("access_token");
  const data = await requestJson("/userinfo", {
    method: "GET",
    headers: { Authorization: `Bearer ${token}` },
  });
  logJson("Userinfo", data);
}

async function handleIntrospect() {
  saveTokenFields();
  const token = storage.get("access_token");
  const data = await requestForm("/introspect", { token });
  logJson("Introspect", data);
}

async function handleRevoke() {
  saveTokenFields();
  const token = storage.get("refresh_token");
  const data = await requestForm("/revoke", { token });
  logJson("Revoke", data);
}

async function handleTokenExchange() {
  const code = storage.get("last_code");
  const verifier = storage.get("pkce_verifier");
  const payload = {
    grant_type: "authorization_code",
    code,
    redirect_uri: $("redirectUri").value.trim(),
    code_verifier: verifier,
    client_id: $("clientId").value.trim(),
    client_secret: $("clientSecret").value.trim(),
  };
  const data = await requestForm("/token", payload);
  storage.set("access_token", data.access_token);
  storage.set("refresh_token", data.refresh_token || "");
  storage.set("id_token", data.id_token || "");
  updateTokenFields();
  logJson("Token exchange", data);
}

async function startAuthorizeFlow() {
  const { verifier, challenge } = await generatePkce();
  const clientId = $("clientId").value.trim();
  const scope = $("scope").value.trim();
  const redirectUri = $("redirectUri").value.trim();
  const state = crypto.randomUUID();
  const nonce = crypto.randomUUID();

  storage.set("pkce_verifier", verifier);
  storage.set("oauth_client_id", clientId);
  storage.set("oauth_client_secret", $("clientSecret").value.trim());
  storage.set("oauth_scope", scope);
  storage.set("oauth_redirect", redirectUri);

  const params = new URLSearchParams({
    response_type: "code",
    client_id: clientId,
    redirect_uri: redirectUri,
    scope,
    state,
    code_challenge: challenge,
    code_challenge_method: "S256",
    nonce,
  });

  const url = `${getBaseUrl()}/authorize?${params.toString()}`;
  logMessage(`Authorize URL: ${url}`);

  if ($("openNewTab").checked) {
    window.open(url, "_blank", "noopener");
  } else {
    window.location.href = url;
  }
}

function initDefaults() {
  const redirectUri = new URL("callback.html", window.location.href).toString();
  if ($("redirectUri")) {
    $("redirectUri").value = storage.get("oauth_redirect", redirectUri);
  }
  if ($("clientId")) {
    $("clientId").value = storage.get("oauth_client_id", "demo-client");
  }
  if ($("clientSecret")) {
    $("clientSecret").value = storage.get("oauth_client_secret", "demo-secret");
  }
  if ($("scope")) {
    $("scope").value = storage.get("oauth_scope", "openid profile email offline_access");
  }
  updateTokenFields();
}

async function handleCallback() {
  const params = new URLSearchParams(window.location.search);
  const code = params.get("code");
  const error = params.get("error");

  if (error) {
    logMessage(`OAuth error: ${error}`);
    return;
  }

  if (!code) {
    logMessage("No code in callback URL.");
    return;
  }

  storage.set("last_code", code);
  const payload = {
    grant_type: "authorization_code",
    code,
    redirect_uri: storage.get(
      "oauth_redirect",
      new URL("callback.html", window.location.href).toString()
    ),
    code_verifier: storage.get("pkce_verifier"),
    client_id: storage.get("oauth_client_id"),
    client_secret: storage.get("oauth_client_secret"),
  };

  try {
    const data = await requestForm("/token", payload);
    storage.set("access_token", data.access_token);
    storage.set("refresh_token", data.refresh_token || "");
    storage.set("id_token", data.id_token || "");
    updateTokenFields();
    logJson("Callback token", data);
  } catch (err) {
    logMessage(`Token exchange failed: ${err.message}`);
  }
}

function bindEvents() {
  bindButton("btnHealth", () => handleHealth().catch(handleError));
  bindButton("btnOpenId", () => handleOpenId().catch(handleError));
  bindButton("btnRegister", () => handleRegister().catch(handleError));
  bindButton("btnLoginJson", () => handleLogin().catch(handleError));
  bindButton("btnLogin", openLoginModal);
  bindButton("btnLogout", () => handleLogout().catch(handleError));
  bindButton("btnRefresh", () => handleRefresh().catch(handleError));
  bindButton("btnClearTokens", () => {
    storage.clear(["access_token", "refresh_token", "id_token"]);
    updateTokenFields();
    logMessage("Tokens limpiados.");
  });
  bindButton("btnStartAuth", () => startAuthorizeFlow().catch(handleError));
  bindButton("btnToken", () => handleTokenExchange().catch(handleError));
  bindButton("btnDashboard", () => handleDashboard().catch(handleError));
  bindButton("btnUserinfo", () => handleUserinfo().catch(handleError));
  bindButton("btnIntrospect", () => handleIntrospect().catch(handleError));
  bindButton("btnRevoke", () => handleRevoke().catch(handleError));
  bindButton("btnMetrics", () => handleMetrics().catch(handleError));
  bindButton("btnUsers", () => handleUsers().catch(handleError));
  bindButton("btnJwks", () => handleJwks().catch(handleError));
  bindButton("btnRoot", () => handleRoot().catch(handleError));

  const overlay = document.querySelector("[data-close]");
  if (overlay) overlay.addEventListener("click", closeLoginModal);
  bindButton("btnCloseModal", closeLoginModal);
}

function handleError(err) {
  logMessage(`Error: ${err.message}`);
}

document.addEventListener("DOMContentLoaded", () => {
  initDefaults();
  bindEvents();

  if (window.location.pathname.endsWith("callback.html")) {
    handleCallback();
  }
});

window.addEventListener("message", (event) => {
  const baseUrl = getBaseUrl();
  if (event.origin !== baseUrl) return;
  if (event.data && event.data.type === "sso-login-token") {
    storage.set("access_token", event.data.access_token || "");
    storage.set("refresh_token", event.data.refresh_token || "");
    storage.set("id_token", event.data.id_token || "");
    updateTokenFields();
    window.location.href = "private.html";
    return;
  }
  if (event.data && event.data.type === "sso-login-error") {
    const errorBox = $("loginError");
    if (errorBox) {
      errorBox.textContent = event.data.message || "Error de login";
      errorBox.classList.remove("hidden");
    }
  }
});
