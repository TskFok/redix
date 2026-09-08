import { useCallback, useState, type SetStateAction } from "react";

/** 每次反馈都有独立标识，让同一错误再次发生时也能重新显示并计时。 */
export function useFeedbackState<T>(initial: T | (() => T)) {
  const [feedback, setFeedback] = useState(() => ({
    value: typeof initial === "function" ? (initial as () => T)() : initial,
    token: {},
  }));
  const setValue = useCallback((next: SetStateAction<T>) => {
    setFeedback((current) => ({
      value: typeof next === "function" ? (next as (value: T) => T)(current.value) : next,
      token: {},
    }));
  }, []);
  return [feedback.value, setValue, feedback.token] as const;
}
