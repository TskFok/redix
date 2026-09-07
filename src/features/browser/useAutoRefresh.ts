import { useEffect, useRef } from "react";
export function useAutoRefresh(seconds: number, busy: boolean, refresh: () => void) {
  const latest = useRef({ busy, refresh });
  latest.current = { busy, refresh };
  useEffect(() => {
    if (![2, 5, 10, 30].includes(seconds)) return;
    const timer = window.setInterval(() => {
      if (!latest.current.busy && document.visibilityState !== "hidden") latest.current.refresh();
    }, seconds * 1000);
    return () => window.clearInterval(timer);
  }, [seconds]);
}
