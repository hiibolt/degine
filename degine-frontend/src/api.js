const runtime = window.degine || {};

export const apiUrl = runtime.apiUrl || import.meta.env.VITE_API_URL || "";
export const wsUrl = runtime.wsUrl || import.meta.env.VITE_WS_URL || "";

export class ApiError extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

export async function api(path, { method = "GET", token, body } = {}) {
  const headers = {};
  if (token) headers.authorization = `Bearer ${token}`;
  if (body !== undefined) headers["content-type"] = "application/json";
  let response;
  try {
    response = await fetch(`${apiUrl}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new ApiError(0, "can't reach the server");
  }
  const text = await response.text();
  let data = null;
  if (text) {
    try {
      data = JSON.parse(text);
    } catch {
      data = null;
    }
  }
  if (!response.ok) {
    throw new ApiError(response.status, data?.error || "request failed");
  }
  return data;
}

export function connectEvents(token, onEvent, onStatus) {
  let stopped = false;
  let socket = null;
  let timer = null;
  let delay = 500;
  let everOpened = false;

  const open = () => {
    if (stopped) return;
    onStatus("connecting");
    let url;
    const address =
      wsUrl ||
      `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/ws`;
    try {
      url = new URL(address);
    } catch {
      onStatus("closed");
      return;
    }
    url.searchParams.set("token", token);
    socket = new WebSocket(url);
    socket.onopen = () => {
      delay = 500;
      onStatus("open");
      if (everOpened) onEvent({ type: "reconnected" });
      everOpened = true;
    };
    socket.onmessage = (message) => {
      try {
        onEvent(JSON.parse(message.data));
      } catch {
        // A bad frame should not take the socket down.
      }
    };
    socket.onclose = () => {
      onStatus("closed");
      if (!stopped) {
        timer = setTimeout(open, delay);
        delay = Math.min(delay * 2, 8000);
      }
    };
    socket.onerror = () => socket.close();
  };

  open();
  return () => {
    stopped = true;
    clearTimeout(timer);
    socket?.close();
  };
}
