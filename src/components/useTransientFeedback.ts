import { useCallback, useEffect, useState } from "react";

interface Feedback<T> {
  value: T;
  durationMs: number | null;
}

/** 操作反馈默认显示 3 秒；需要用户处理的错误或警告可用 null 保持显示。 */
export function useTransientFeedback<T = string>() {
  const [feedback, setFeedback] = useState<Feedback<T> | null>(null);

  const showFeedback = useCallback((value: T | null, durationMs: number | null = 3000) => {
    // 每次创建新记录，让内容相同的连续操作也能重新计时。
    setFeedback(value === null ? null : { value, durationMs });
  }, []);

  useEffect(() => {
    if (feedback === null || feedback.durationMs === null) return;
    const timer = window.setTimeout(() => {
      setFeedback((current) => current === feedback ? null : current);
    }, feedback.durationMs);
    return () => window.clearTimeout(timer);
  }, [feedback]);

  return [feedback === null ? null : feedback.value, showFeedback] as const;
}
