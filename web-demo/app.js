const $ = (id) => document.getElementById(id);

const STORAGE_KEYS = {
  access: "access_token",
  refresh: "refresh_token",
  id: "id_token",
  base: "base_url",
  verifier: "pkce_verifier",
  state: "oauth_state",
  nonce: "oauth_nonce",
};

const CLIENT_ID = "demo-client";
const CLIENT_SECRET = "demo-secret";
const DEFAULT_SCOPE = "openid profile email offline_access dashboard.read";
const DEFAULT_BASE_URL = "http://localhost:3000";

const storage = {
  get: (key, fallback = "") => sessionStorage.getItem(key) || fallback,
  set: (key, value) => sessionStorage.setItem(key, value || ""),
  clear: (keys) => keys.forEach((key) => sessionStorage.removeItem(key)),
};

function normalizeBaseUrl(value) {
  let base = value.trim();
  if (!/^https?:\/\//i.test(base)) {
    base = `http://${base}`;
  }
  return base.replace(/\/$/, "");
}

function getBaseUrl() {
  const params = new URLSearchParams(window.location.search);
  const override = params.get("base");
  if (override) {
    const normalized = normalizeBaseUrl(override);
    storage.set(STORAGE_KEYS.base, normalized);
  }
  return normalizeBaseUrl(storage.get(STORAGE_KEYS.base, DEFAULT_BASE_URL));
}

function setError(message) {
  const box = $("loginError");
  if (!box) return;
  box.textContent = message;
  box.classList.remove("hidden");
}

function setPrivateError(message) {
  const box = $("privateError");
  if (!box) return;
  box.textContent = message;
  box.classList.remove("hidden");
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

async function requestForm(path, body) {
  const url = `${getBaseUrl()}${path}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams(body),
    credentials: "include",
  });
  const text = await response.text();
  const isJson = text.startsWith("{") || text.startsWith("[");
  const data = isJson ? JSON.parse(text) : text;
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}: ${JSON.stringify(data)}`);
  }
  return data;
}

function startAuthorize() {
  const redirectUri = new URL("callback.html", window.location.href).toString();
  const state = crypto.randomUUID();
  const nonce = crypto.randomUUID();

  storage.set(STORAGE_KEYS.state, state);
  storage.set(STORAGE_KEYS.nonce, nonce);

  generatePkce().then(({ verifier, challenge }) => {
    storage.set(STORAGE_KEYS.verifier, verifier);
    const params = new URLSearchParams({
      response_type: "code",
      client_id: CLIENT_ID,
      redirect_uri: redirectUri,
      scope: DEFAULT_SCOPE,
      state,
      code_challenge: challenge,
      code_challenge_method: "S256",
      nonce,
    });
    window.location.href = `${getBaseUrl()}/authorize?${params.toString()}`;
  });
}

async function handleCallback() {
  const params = new URLSearchParams(window.location.search);
  const code = params.get("code");
  const state = params.get("state");
  const storedState = storage.get(STORAGE_KEYS.state);

  if (!code) {
    setError("No se recibió code en el callback.");
    return;
  }
  if (!state || state !== storedState) {
    setError("State inválido. Repite el login.");
    return;
  }

  try {
    const redirectUri = new URL("callback.html", window.location.href).toString();
    const data = await requestForm("/token", {
      grant_type: "authorization_code",
      code,
      redirect_uri: redirectUri,
      code_verifier: storage.get(STORAGE_KEYS.verifier),
      client_id: CLIENT_ID,
      client_secret: CLIENT_SECRET,
    });
    storage.set(STORAGE_KEYS.access, data.access_token || "");
    storage.set(STORAGE_KEYS.refresh, data.refresh_token || "");
    storage.set(STORAGE_KEYS.id, data.id_token || "");
    window.location.href = "private.html";
  } catch (err) {
    setError(err.message || "Error al intercambiar el token.");
  }
}

function decodeJwt(token) {
  const parts = token.split(".");
  if (parts.length < 2) return null;
  const payload = parts[1]
    .replace(/-/g, "+")
    .replace(/_/g, "/")
    .padEnd(parts[1].length + (4 - (parts[1].length % 4)) % 4, "=");
  try {
    return JSON.parse(atob(payload));
  } catch (err) {
    return null;
  }
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

function setText(id, value) {
  const el = $(id);
  if (el) el.textContent = value;
}

function redirectToLogin() {
  storage.clear([STORAGE_KEYS.access, STORAGE_KEYS.refresh, STORAGE_KEYS.id]);
  window.location.href = "index.html";
}

async function initPrivate() {
  const token = storage.get(STORAGE_KEYS.access);
  if (!token) {
    setPrivateError("No hay access token en sessionStorage.");
    return;
  }

  const logoutBtn = $("btnLogout");
  if (logoutBtn) {
    logoutBtn.addEventListener("click", async () => {
      try {
        await requestJson("/logout/all", {
          method: "POST",
          headers: { Authorization: `Bearer ${token}` },
        });
      } catch (err) {
        // ignore
      }
      redirectToLogin();
    });
  }

  let userinfo;
  try {
    userinfo = await requestJson("/userinfo", {
      method: "GET",
      headers: { Authorization: `Bearer ${token}` },
    });
    setText("userinfoStatus", "OK");
  } catch (err) {
    setText("userinfoStatus", "No autorizado");
    setPrivateError(`Fallo /userinfo: ${err.message}`);
  }

  const claims = decodeJwt(token) || {};
  const userinfoData = userinfo || {};
  const scopes = (claims.scope || "").split(/\s+/).filter(Boolean);
  const scopeList = $("scopeList");
  if (scopeList) {
    scopeList.innerHTML = "";
    if (scopes.length === 0) {
      const li = document.createElement("li");
      li.textContent = "sin scopes";
      scopeList.appendChild(li);
    } else {
      scopes.forEach((scope) => {
        const li = document.createElement("li");
        li.textContent = scope;
        scopeList.appendChild(li);
      });
    }
  }

  setText("userName", userinfoData.preferred_username || userinfoData.sub || "-");
  setText("userEmail", userinfoData.email || "-");
  setText("userRole", userinfoData.role || claims.role || "-");
  setText("clientId", claims.aud || "-");
  setText("audienceInfo", `audiencia: ${claims.aud || "-"}`);

  if (claims.exp) {
    const expDate = new Date(claims.exp * 1000);
    setText("expiresInfo", `expira: ${expDate.toLocaleString()}`);
  }

  try {
    await requestJson("/dashboard", {
      method: "GET",
      headers: { Authorization: `Bearer ${token}` },
    });
    setText("dashboardStatus", "OK");
    setText("accessStatus", "Acceso verificado");
    $("accessStatus")?.classList.remove("error");
    $("accessStatus")?.classList.add("ok");
  } catch (err) {
    setText("dashboardStatus", "Sin acceso");
    setText("accessStatus", "Acceso denegado");
    $("accessStatus")?.classList.remove("ok");
    $("accessStatus")?.classList.add("error");
    setPrivateError(`Fallo /dashboard: ${err.message}`);
  }

}

document.addEventListener("DOMContentLoaded", () => {
  if (window.location.pathname.endsWith("callback.html")) {
    handleCallback();
    return;
  }
  if (window.location.pathname.endsWith("private.html")) {
    initPrivate();
    return;
  }

  const btn = $("btnStartAuth");
  if (btn) {
    btn.addEventListener("click", startAuthorize);
  } else {
    startAuthorize();
  }
});
