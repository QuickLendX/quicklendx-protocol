/// <reference types="@testing-library/jest-dom" />

/**
 * Jest setup for the jsdom project.
 *
 * `@testing-library/jest-dom` adds the DOM matchers used by the component
 * tests (`toBeInTheDocument`, `toHaveTextContent`, ...).
 */
require("@testing-library/jest-dom");

// Toasts are mounted by `react-hot-toast` on the next tick after they are
// emitted, so the default 1s `findBy*` timeout is occasionally too tight.
const { configure } = require("@testing-library/react");
configure({ asyncUtilTimeout: 5000 });

// jsdom does not implement `window.matchMedia`, but `react-hot-toast` queries
// it to decide whether to animate the toast stack.
if (typeof window.matchMedia !== "function") {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    configurable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => undefined,
      removeListener: () => undefined,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      dispatchEvent: () => false,
    }),
  });
}
