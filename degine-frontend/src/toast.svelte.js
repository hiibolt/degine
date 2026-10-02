export const toasts = $state([]);

let next = 1;

export function pushToast(message, tone = "plain") {
  const id = next;
  next += 1;
  toasts.push({ id, message, tone });
  setTimeout(() => dismiss(id), 4200);
}

export function dismiss(id) {
  const index = toasts.findIndex((toast) => toast.id === id);
  if (index >= 0) toasts.splice(index, 1);
}
