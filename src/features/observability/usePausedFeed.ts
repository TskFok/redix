import { useState } from "react";

export function usePausedFeed<T>(items: T[], clear: () => void) {
  const [snapshot, setSnapshot] = useState<T[] | null>(null);
  return {
    paused: snapshot !== null,
    displayed: snapshot ?? items,
    toggle: () => setSnapshot((current) => current === null ? items : null),
    clear: () => {
      setSnapshot((current) => current === null ? null : []);
      clear();
    },
  };
}
