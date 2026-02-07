const handlers = new Map<string, () => void>();

export function registerAction(name: string, handler: () => void): void {
  handlers.set(name, handler);
}

export function unregisterAction(name: string): void {
  handlers.delete(name);
}

export function dispatchAction(name: string): boolean {
  const handler = handlers.get(name);
  if (handler) {
    handler();
    return true;
  }
  return false;
}
