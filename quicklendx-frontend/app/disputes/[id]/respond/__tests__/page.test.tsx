import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import DisputeResponsePage from "../page";

let mockParams: Record<string, string | undefined> = {
  id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
};
const mockNotFound = jest.fn(() => {
  throw new Error("NEXT_NOT_FOUND");
});

jest.mock("next/navigation", () => ({
  useParams: () => mockParams,
  useRouter: () => ({ push: jest.fn(), replace: jest.fn(), prefetch: jest.fn() }),
  usePathname: () => "/disputes/0x1234/respond",
  useSearchParams: () => new URLSearchParams(),
  notFound: () => mockNotFound(),
}));

describe("DisputeResponsePage", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockParams = {
      id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    };
  });

  it("renders page with valid hex dispute ID", () => {
    render(<DisputeResponsePage />);

    expect(
      screen.getByText(
        `Respond to Dispute #${mockParams.id}`
      )
    ).toBeInTheDocument();
    expect(screen.getByText("How would you like to respond?")).toBeInTheDocument();
    expect(screen.getByLabelText("Agree & Settle")).toBeInTheDocument();
    expect(screen.getByLabelText("Disagree & Provide Evidence")).toBeInTheDocument();
    expect(screen.getByLabelText("Request More Information")).toBeInTheDocument();
    expect(screen.getByLabelText("Escalate to Mediator")).toBeInTheDocument();
  });

  it("calls notFound() when dispute ID is not a valid hex string", () => {
    const errorSpy = jest.spyOn(console, "error").mockImplementation(() => {});
    mockParams = { id: "invalid-id" };

    expect(() => render(<DisputeResponsePage />)).toThrow("NEXT_NOT_FOUND");
    expect(mockNotFound).toHaveBeenCalled();
    errorSpy.mockRestore();
  });

  it("renders fallback ID when dispute ID is undefined", () => {
    mockParams = { id: undefined };
    render(<DisputeResponsePage />);

    expect(
      screen.getByText("Respond to Dispute #DSP-20260426-001")
    ).toBeInTheDocument();
    expect(mockNotFound).not.toHaveBeenCalled();
  });

  it("opens confirmation dialog when option is selected and submit is clicked", () => {
    render(<DisputeResponsePage />);

    const submitButton = screen.getByRole("button", { name: "Submit Response" });
    expect(submitButton).toBeDisabled();

    // Select an option
    const agreeOption = screen.getByLabelText("Agree & Settle");
    fireEvent.click(agreeOption);

    expect(submitButton).not.toBeDisabled();

    // Click Submit Response
    fireEvent.click(submitButton);

    // Confirmation dialog should be visible
    expect(
      screen.getByText("Confirm Response Submission?")
    ).toBeInTheDocument();
    expect(
      screen.getByText(/You are about to submit your response:/)
    ).toBeInTheDocument();

    // Click Cancel to dismiss dialog
    const cancelButton = screen.getByRole("button", { name: "Cancel" });
    fireEvent.click(cancelButton);

    expect(
      screen.queryByText("Confirm Response Submission?")
    ).not.toBeInTheDocument();
  });
});
