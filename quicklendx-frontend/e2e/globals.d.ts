export {};

declare global {
  interface Window {
    __QLX_TEST_WALLET__?: string;
    __TEST_WALLET_ADDRESS__?: string;
  }
}
