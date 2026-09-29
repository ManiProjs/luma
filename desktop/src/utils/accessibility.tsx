import { useEffect, useRef } from 'react';

// Screen reader only text wrapper
export function ScreenReaderOnly({ children }: { children: React.ReactNode }) {
  return (
    <span
      className="sr-only"
      style={{
        position: 'absolute',
        width: '1px',
        height: '1px',
        padding: 0,
        margin: '-1px',
        overflow: 'hidden',
        clip: 'rect(0, 0, 0, 0)',
        whiteSpace: 'nowrap',
        border: 0,
      }}
    >
      {children}
    </span>
  );
}

// Live region announcer hook
export function useAnnounce(
  message: string,
  priority: 'polite' | 'assertive' = 'polite'
) {
  useEffect(() => {
    if (!message) return;
    const region = document.getElementById('luma-announcer');
    if (region) {
      region.setAttribute('aria-live', priority);
      region.textContent = message;
      // Clear after announcement
      setTimeout(() => {
        region.textContent = '';
      }, 100);
    }
  }, [message, priority]);
}

// Create an announcer element attached to body
export function createAnnouncer() {
  if (document.getElementById('luma-announcer')) return;

  const announcer = document.createElement('div');
  announcer.id = 'luma-announcer';
  announcer.setAttribute('aria-live', 'polite');
  announcer.setAttribute('aria-atomic', 'true');
  announcer.style.cssText = `
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  `;
  document.body.appendChild(announcer);
}

// Announce a message immediately
export function announce(message: string, priority: 'polite' | 'assertive' = 'polite') {
  if (!message) return;

  createAnnouncer();

  const region = document.getElementById('luma-announcer');
  if (region) {
    region.setAttribute('aria-live', priority);
    region.textContent = message;
    // Clear after announcement
    setTimeout(() => {
      region.textContent = '';
    }, 100);
  }
}