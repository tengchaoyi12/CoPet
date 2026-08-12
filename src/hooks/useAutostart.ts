import { useEffect, useState } from "react";
import { toast } from "sonner";

import {
  getAutostartEnabled,
  setAutostartEnabled,
} from "../lib/appCommands";

export function useAutostart(active: boolean) {
  const [enabled, setEnabled] = useState(false);
  const [loading, setLoading] = useState(false);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    setLoading(true);
    void getAutostartEnabled().then((result) => {
      if (cancelled) return;
      setLoading(false);
      if (result.errorMessage) {
        toast.error(result.errorMessage);
        return;
      }
      setEnabled(result.enabled ?? false);
    });
    return () => {
      cancelled = true;
    };
  }, [active]);

  const update = async (next: boolean) => {
    if (pending) return;
    setPending(true);
    const result = await setAutostartEnabled(next);
    setPending(false);
    if (result.errorMessage) {
      toast.error(result.errorMessage);
      return;
    }
    setEnabled(result.enabled ?? enabled);
  };

  return { enabled, loading, pending, update };
}
