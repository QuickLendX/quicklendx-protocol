import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import toast from "react-hot-toast";
import NotificationPreferencesPage from "../page";
import {
  getNotificationPreferences,
  updateNotificationPreferences,
  NotificationType,
} from "../_lib/data";
import { useWallet } from "@/app/lib/wallet/WalletProvider";

jest.mock("react-hot-toast", () => ({
  __esModule: true,
  default: { error: jest.fn(), success: jest.fn() },
}));

jest.mock("@/app/lib/wallet/WalletProvider", () => ({
  useWallet: jest.fn(),
}));

jest.mock("../_lib/data", () => ({
  ...jest.requireActual("../_lib/data"),
  getNotificationPreferences: jest.fn(),
  updateNotificationPreferences: jest.fn(),
}));

const mockGet = jest.mocked(getNotificationPreferences);
const mockUpdate = jest.mocked(updateNotificationPreferences);
const mockWallet = jest.mocked(useWallet);
const walletAddress = "GABCDEXAMPLESTELLARPUBLICKEY000000000000000000000000000";

const preferences = {
  email_enabled: true,
  email_address: "investor@example.com",
  notifications: {
    [NotificationType.InvoiceFunded]: true,
    [NotificationType.PaymentReceived]: false,
    [NotificationType.DisputeOpened]: true,
    [NotificationType.DisputeResolved]: false,
  },
};

describe("NotificationPreferencesPage", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockWallet.mockReturnValue({
      state: "CONNECTED",
      address: walletAddress,
      connect: jest.fn(),
      disconnect: jest.fn(),
      signTransaction: jest.fn(),
    });
    mockGet.mockResolvedValue(preferences);
    mockUpdate.mockResolvedValue({ success: true });
  });

  it("loads the wallet's preferences and persists changes", async () => {
    render(<NotificationPreferencesPage />);

    expect(await screen.findByLabelText("Invoice funded")).toBeChecked();
    expect(screen.getByLabelText("Payment received")).not.toBeChecked();
    expect(screen.getByLabelText("Email address")).toHaveValue("investor@example.com");

    fireEvent.click(screen.getByLabelText("Payment received"));
    fireEvent.click(screen.getByRole("button", { name: "Save preferences" }));

    await waitFor(() => {
      expect(mockUpdate).toHaveBeenCalledWith(walletAddress, {
        email_enabled: true,
        email_address: "investor@example.com",
        notifications: {
          ...preferences.notifications,
          [NotificationType.PaymentReceived]: true,
        },
      });
    });
    expect(toast.success).toHaveBeenCalledWith("Notification preferences saved.");
  });

  it("shows a permission error when the API rejects a save with 403", async () => {
    mockUpdate.mockRejectedValue(new Error("You do not have permission to perform this action."));
    render(<NotificationPreferencesPage />);

    await screen.findByLabelText("Invoice funded");
    fireEvent.click(screen.getByLabelText("Payment received"));
    fireEvent.click(screen.getByRole("button", { name: "Save preferences" }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith(
        "You do not have permission to perform this action."
      );
    });
    expect(screen.getByLabelText("Payment received")).not.toBeChecked();
  });

  it("rejects an invalid email before sending an update", async () => {
    render(<NotificationPreferencesPage />);

    await screen.findByLabelText("Invoice funded");
    fireEvent.change(screen.getByLabelText("Email address"), {
      target: { value: "not-an-email" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save preferences" }));

    expect(await screen.findByText("Enter a valid email address.")).toBeInTheDocument();
    expect(mockUpdate).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalledWith("Enter a valid email address.");
  });

  it("uses defaults when a wallet has no saved preferences yet", async () => {
    const notFound = Object.assign(new Error("User preferences not found"), {
      code: "NOT_FOUND",
    });
    mockGet.mockRejectedValue(notFound);

    render(<NotificationPreferencesPage />);

    expect(await screen.findByLabelText("Invoice funded")).toBeChecked();
    expect(screen.getByLabelText("Enable email notifications")).toBeChecked();
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("asks the user to connect a wallet before showing preferences", () => {
    mockWallet.mockReturnValue({
      state: "NOT_CONNECTED",
      address: undefined,
      connect: jest.fn(),
      disconnect: jest.fn(),
      signTransaction: jest.fn(),
    });

    render(<NotificationPreferencesPage />);

    expect(screen.getByRole("status")).toHaveTextContent(
      "Connect your wallet to manage notification preferences."
    );
    expect(mockGet).not.toHaveBeenCalled();
  });
});
