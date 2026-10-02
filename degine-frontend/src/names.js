const key = "degine.usernames";

function read() {
  try {
    const parsed = JSON.parse(localStorage.getItem(key) || "{}");
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

export function rememberName(username, email) {
  const names = read();
  names[username] = email;
  localStorage.setItem(key, JSON.stringify(names));
}

export function emailForName(username) {
  return read()[username] || "";
}
