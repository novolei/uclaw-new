import { useEffect, useRef, useCallback, useInsertionEffect } from "react";

function useEffectEvent<T extends (...args: any[]) => any>(fn: T): T {
  const ref = useRef<T>(fn);
  useInsertionEffect(() => {
    ref.current = fn;
  }, [fn]);
  return useCallback((...args: Parameters<T>) => {
    return ref.current(...args);
  }, []) as T;
}

type SubscribeFn<TPayload> = (handler: (payload: TPayload) => void) => Promise<() => void>;

interface UseTauriSubscriptionOptions {
  enabled?: boolean;
}

export function useTauriSubscription<TPayload>(
  subscribe: SubscribeFn<TPayload>,
  handler: (payload: TPayload) => void,
  options: UseTauriSubscriptionOptions = {},
) {
  const { enabled = true } = options;
  const onEvent = useEffectEvent(handler);

  useEffect(() => {
    if (!enabled) {
      return undefined;
    }

    let active = true;
    const unlistenPromise = subscribe((payload) => {
      if (active) {
        onEvent(payload);
      }
    });

    return () => {
      active = false;
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [enabled, subscribe]);
}
