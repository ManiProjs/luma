import { useEffect, useRef } from 'react';

export function useFocusTrap(active: boolean) {
  const containerRef = useRef<HTMLElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!active || !containerRef.current) return;
    const container = containerRef.current;

    // Store previous focus
    previousFocus.current = document.activeElement as HTMLElement;

    // Prefer an explicit autofocus target, otherwise use the first control.
    const focusables = getFocusables(container);
    const initialFocus = focusables.find((element) => element.hasAttribute('autofocus')) ?? focusables[0];
    initialFocus?.focus();

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== 'Tab') return;

      const focusables = getFocusables(container);
      const first = focusables[0];
      const last = focusables[focusables.length - 1];

      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last?.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first?.focus();
      }
    };

    container.addEventListener('keydown', handleKeyDown);
    return () => {
      container.removeEventListener('keydown', handleKeyDown);
      previousFocus.current?.focus();
    };
  }, [active]);

  return containerRef;
}

function getFocusables(el: HTMLElement): HTMLElement[] {
  return Array.from(
    el.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
    )
  ).filter(el => !el.hasAttribute('disabled') && el.offsetParent !== null);
}
