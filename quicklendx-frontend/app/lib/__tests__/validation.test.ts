import { z } from "zod";
import {
  emailSchema,
  passwordSchema,
  usernameSchema,
  phoneSchema,
  urlSchema,
  amountSchema,
  percentageSchema,
  invoiceSchema,
  bidSchema,
  userProfileSchema,
  loginSchema,
  registrationSchema,
  fileUploadSchema,
  FormValidator,
  useValidation,
  schemas,
} from "../validation";
import { ValidationError } from "../errors";

const VALID_UUID = "123e4567-e89b-12d3-a456-426614174000";
const ANOTHER_UUID = "223e4567-e89b-12d3-a456-426614174001";

function firstMessage(schema: z.ZodTypeAny, data: unknown): string | undefined {
  const result = schema.safeParse(data);
  return result.success ? undefined : result.error.errors[0]?.message;
}

describe("emailSchema", () => {
  it("accepts a valid email", () => {
    expect(emailSchema.safeParse("user@example.com").success).toBe(true);
  });
  it("rejects a malformed email", () => {
    expect(firstMessage(emailSchema, "not-an-email")).toBe(
      "Please enter a valid email address"
    );
  });
  it("rejects an empty string", () => {
    expect(emailSchema.safeParse("").success).toBe(false);
  });
  it("enforces the 254 character maximum at the boundary", () => {
    const local = "a".repeat(64);
    const domain = `${"b".repeat(60)}.${"c".repeat(60)}.${"d".repeat(60)}.com`;
    const tooLong = `${local}@${domain}`;
    expect(tooLong.length).toBeGreaterThan(254);
    expect(firstMessage(emailSchema, tooLong)).toBe("Email is too long");
  });
});

describe("passwordSchema", () => {
  it("accepts a compliant password", () => {
    expect(passwordSchema.safeParse("Passw0rdX").success).toBe(true);
  });
  it("rejects a password shorter than 8 characters", () => {
    expect(firstMessage(passwordSchema, "Pa1")).toBe(
      "Password must be at least 8 characters long"
    );
  });
  it("accepts exactly 8 characters meeting complexity", () => {
    expect(passwordSchema.safeParse("Abcdefg1").success).toBe(true);
  });
  it("rejects a password missing an uppercase letter", () => {
    expect(firstMessage(passwordSchema, "password1")).toMatch(
      /uppercase letter, one lowercase letter, and one number/
    );
  });
  it("rejects a password longer than 128 characters", () => {
    const pw = "Aa1" + "x".repeat(126);
    expect(pw.length).toBe(129);
    expect(firstMessage(passwordSchema, pw)).toBe("Password is too long");
  });
});

describe("usernameSchema", () => {
  it("accepts a valid username", () => {
    expect(usernameSchema.safeParse("john_doe-1").success).toBe(true);
  });
  it("rejects a username shorter than 3 characters", () => {
    expect(firstMessage(usernameSchema, "ab")).toBe(
      "Username must be at least 3 characters long"
    );
  });
  it("rejects a username with invalid characters", () => {
    expect(firstMessage(usernameSchema, "bad name!")).toMatch(
      /can only contain letters, numbers/
    );
  });
  it("enforces the 30 character maximum at the boundary", () => {
    expect(usernameSchema.safeParse("a".repeat(30)).success).toBe(true);
    expect(firstMessage(usernameSchema, "a".repeat(31))).toBe(
      "Username is too long"
    );
  });
});

describe("phoneSchema", () => {
  it("accepts a formatted phone number", () => {
    expect(phoneSchema.safeParse("+1 (555) 123-4567").success).toBe(true);
  });
  it("rejects letters in a phone number", () => {
    expect(firstMessage(phoneSchema, "555-CALL-NOW")).toBe(
      "Please enter a valid phone number"
    );
  });
  it("rejects a number shorter than 10 characters at the boundary", () => {
    expect(firstMessage(phoneSchema, "123456789")).toBe(
      "Phone number is too short"
    );
    expect(phoneSchema.safeParse("1234567890").success).toBe(true);
  });
  it("rejects a number longer than 20 characters", () => {
    expect(firstMessage(phoneSchema, "1".repeat(21))).toBe(
      "Phone number is too long"
    );
  });
});

describe("urlSchema", () => {
  it("accepts a valid URL", () => {
    expect(urlSchema.safeParse("https://example.com").success).toBe(true);
  });
  it("accepts undefined since the field is optional", () => {
    expect(urlSchema.safeParse(undefined).success).toBe(true);
  });
  it("rejects an invalid URL", () => {
    expect(firstMessage(urlSchema, "not a url")).toBe("Please enter a valid URL");
  });
});

describe("amountSchema", () => {
  it("accepts a typical amount", () => {
    expect(amountSchema.safeParse(100.5).success).toBe(true);
  });
  it("rejects zero", () => {
    expect(firstMessage(amountSchema, 0)).toBe("Amount must be positive");
  });
  it("rejects a negative amount", () => {
    expect(firstMessage(amountSchema, -5)).toBe("Amount must be positive");
  });
  it("accepts the minimum boundary 0.01", () => {
    expect(amountSchema.safeParse(0.01).success).toBe(true);
  });
  it("rejects a value below the 0.01 minimum", () => {
    expect(firstMessage(amountSchema, 0.001)).toBe(
      "Amount must be at least 0.01"
    );
  });
  it("accepts the maximum boundary 999999999.99", () => {
    expect(amountSchema.safeParse(999999999.99).success).toBe(true);
  });
  it("rejects a value above the maximum", () => {
    expect(firstMessage(amountSchema, 1000000000)).toBe("Amount is too large");
  });
});

describe("percentageSchema", () => {
  it("accepts a value within range", () => {
    expect(percentageSchema.safeParse(50).success).toBe(true);
  });
  it("accepts the lower boundary 0", () => {
    expect(percentageSchema.safeParse(0).success).toBe(true);
  });
  it("accepts the upper boundary 100", () => {
    expect(percentageSchema.safeParse(100).success).toBe(true);
  });
  it("rejects a negative percentage", () => {
    expect(firstMessage(percentageSchema, -1)).toBe(
      "Percentage must be at least 0%"
    );
  });
  it("rejects a percentage above 100", () => {
    expect(firstMessage(percentageSchema, 100.01)).toBe(
      "Percentage cannot exceed 100%"
    );
  });
});

describe("invoiceSchema", () => {
  const future = new Date(Date.now() + 7 * 24 * 60 * 60 * 1000);
  const past = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000);
  const valid = {
    id: VALID_UUID,
    amount: 250,
    currency: "USD" as const,
    dueDate: future,
    description: "Consulting services",
  };

  it("accepts a valid invoice", () => {
    expect(invoiceSchema.safeParse(valid).success).toBe(true);
  });
  it("rejects an invalid id", () => {
    const r = invoiceSchema.safeParse({ ...valid, id: "nope" });
    expect(r.success).toBe(false);
    expect(r.success || r.error.errors[0].message).toBe("Invalid invoice ID");
  });
  it("rejects an unsupported currency", () => {
    const r = invoiceSchema.safeParse({ ...valid, currency: "JPY" });
    expect(r.success).toBe(false);
  });
  it("rejects a due date in the past", () => {
    const r = invoiceSchema.safeParse({ ...valid, dueDate: past });
    expect(r.success).toBe(false);
    expect(r.success || r.error.errors[0].message).toBe(
      "Due date must be in the future"
    );
  });
  it("rejects an empty description", () => {
    const r = invoiceSchema.safeParse({ ...valid, description: "" });
    expect(r.success).toBe(false);
    expect(r.success || r.error.errors[0].message).toBe("Description is required");
  });
  it("enforces the 500 character description boundary", () => {
    expect(
      invoiceSchema.safeParse({ ...valid, description: "d".repeat(500) }).success
    ).toBe(true);
    const r = invoiceSchema.safeParse({ ...valid, description: "d".repeat(501) });
    expect(r.success || r.error.errors[0].message).toBe(
      "Description is too long"
    );
  });
});

describe("bidSchema", () => {
  const valid = {
    id: VALID_UUID,
    invoiceId: ANOTHER_UUID,
    amount: 200,
    interestRate: 5,
    term: 30,
  };

  it("accepts a valid bid", () => {
    expect(bidSchema.safeParse(valid).success).toBe(true);
  });
  it("rejects an invalid invoiceId", () => {
    const r = bidSchema.safeParse({ ...valid, invoiceId: "bad" });
    expect(r.success || r.error.errors[0].message).toBe("Invalid invoice ID");
  });
  it("rejects an interest rate above 100", () => {
    expect(bidSchema.safeParse({ ...valid, interestRate: 101 }).success).toBe(
      false
    );
  });
  it("enforces the term minimum boundary of 1 day", () => {
    expect(bidSchema.safeParse({ ...valid, term: 1 }).success).toBe(true);
    const r = bidSchema.safeParse({ ...valid, term: 0 });
    expect(r.success || r.error.errors[0].message).toBe(
      "Term must be at least 1 day"
    );
  });
  it("enforces the term maximum boundary of 365 days", () => {
    expect(bidSchema.safeParse({ ...valid, term: 365 }).success).toBe(true);
    const r = bidSchema.safeParse({ ...valid, term: 366 });
    expect(r.success || r.error.errors[0].message).toBe(
      "Term cannot exceed 365 days"
    );
  });
  it("rejects a non-integer term", () => {
    const r = bidSchema.safeParse({ ...valid, term: 2.5 });
    expect(r.success || r.error.errors[0].message).toBe(
      "Term must be a whole number"
    );
  });
});

describe("userProfileSchema", () => {
  const valid = {
    firstName: "Jane",
    lastName: "Doe",
    email: "jane@example.com",
  };

  it("accepts a minimal valid profile", () => {
    expect(userProfileSchema.safeParse(valid).success).toBe(true);
  });
  it("rejects a first name with digits", () => {
    const r = userProfileSchema.safeParse({ ...valid, firstName: "Jane1" });
    expect(r.success || r.error.errors[0].message).toBe(
      "First name can only contain letters and spaces"
    );
  });
  it("rejects an empty last name", () => {
    const r = userProfileSchema.safeParse({ ...valid, lastName: "" });
    expect(r.success || r.error.errors[0].message).toBe("Last name is required");
  });
  it("enforces the 50 character first name boundary", () => {
    expect(
      userProfileSchema.safeParse({ ...valid, firstName: "a".repeat(50) }).success
    ).toBe(true);
    expect(
      userProfileSchema.safeParse({ ...valid, firstName: "a".repeat(51) }).success
    ).toBe(false);
  });
  it("rejects an invalid website URL", () => {
    expect(
      userProfileSchema.safeParse({ ...valid, website: "nope" }).success
    ).toBe(false);
  });
});

describe("loginSchema", () => {
  it("accepts valid credentials", () => {
    expect(
      loginSchema.safeParse({ email: "a@b.co", password: "x" }).success
    ).toBe(true);
  });
  it("rejects an empty password", () => {
    const r = loginSchema.safeParse({ email: "a@b.co", password: "" });
    expect(r.success || r.error.errors[0].message).toBe("Password is required");
  });
  it("rejects an invalid email", () => {
    expect(
      loginSchema.safeParse({ email: "bad", password: "x" }).success
    ).toBe(false);
  });
});

describe("registrationSchema", () => {
  const valid = {
    email: "new@example.com",
    password: "Passw0rd",
    confirmPassword: "Passw0rd",
    firstName: "A",
    lastName: "B",
    acceptTerms: true,
  };

  it("accepts a valid registration", () => {
    expect(registrationSchema.safeParse(valid).success).toBe(true);
  });
  it("rejects mismatched passwords with the confirmPassword path", () => {
    const r = registrationSchema.safeParse({
      ...valid,
      confirmPassword: "Different1",
    });
    expect(r.success).toBe(false);
    if (!r.success) {
      expect(r.error.errors[0].message).toBe("Passwords don't match");
      expect(r.error.errors[0].path).toEqual(["confirmPassword"]);
    }
  });
  it("rejects when terms are not accepted", () => {
    const r = registrationSchema.safeParse({ ...valid, acceptTerms: false });
    expect(r.success || r.error.errors[0].message).toBe(
      "You must accept the terms and conditions"
    );
  });
});

describe("fileUploadSchema", () => {
  it("accepts a small allowed file", () => {
    const file = new File(["hello"], "doc.pdf", { type: "application/pdf" });
    expect(fileUploadSchema.safeParse({ file }).success).toBe(true);
  });
  it("rejects a disallowed extension", () => {
    const file = new File(["hello"], "script.exe");
    expect(fileUploadSchema.safeParse({ file }).success).toBe(false);
  });
  it("rejects a file exceeding the custom maxSize boundary", () => {
    const file = new File(["12345"], "doc.pdf");
    expect(
      fileUploadSchema.safeParse({ file, maxSize: 4 }).success
    ).toBe(false);
    expect(
      fileUploadSchema.safeParse({ file, maxSize: 5 }).success
    ).toBe(true);
  });
});

describe("FormValidator", () => {
  it("validate returns parsed data for valid input", () => {
    expect(FormValidator.validate(amountSchema, 10)).toBe(10);
  });
  it("validate throws ValidationError with fieldErrors for invalid input", () => {
    expect(() => FormValidator.validate(loginSchema, { email: "x" })).toThrow(
      ValidationError
    );
    try {
      FormValidator.validate(loginSchema, { email: "x", password: "" });
    } catch (e) {
      expect(e).toBeInstanceOf(ValidationError);
      const ctx = (e as ValidationError).context as any;
      expect(ctx.additionalData.fieldErrors.email).toBe(
        "Please enter a valid email address"
      );
    }
  });
  it("validatePartial accepts a subset of fields", () => {
    expect(
      FormValidator.validatePartial(loginSchema, { email: "a@b.co" })
    ).toEqual({ email: "a@b.co" });
  });
  it("validatePartial still rejects an invalid provided field", () => {
    expect(() =>
      FormValidator.validatePartial(loginSchema, { email: "bad" })
    ).toThrow(ValidationError);
  });
  it("validateAsync resolves for valid input", async () => {
    await expect(FormValidator.validateAsync(amountSchema, 5)).resolves.toBe(5);
  });
  it("validateAsync rejects with ValidationError for invalid input", async () => {
    await expect(FormValidator.validateAsync(amountSchema, -1)).rejects.toThrow(
      ValidationError
    );
  });
  it("validateAndSanitize validates sanitized input", () => {
    const result = FormValidator.validateAndSanitize(usernameSchema, "  bob  ");
    expect(typeof result).toBe("string");
  });
});

describe("useValidation", () => {
  const { validate, validateField } = useValidation(loginSchema);

  it("validate reports isValid true with data for valid input", () => {
    const r = validate({ email: "a@b.co", password: "x" });
    expect(r.isValid).toBe(true);
    expect(r.errors).toEqual({});
    expect(r.data).toEqual({ email: "a@b.co", password: "x" });
  });
  it("validate reports field errors for invalid input", () => {
    const r = validate({ email: "bad", password: "" });
    expect(r.isValid).toBe(false);
    expect(r.errors.email).toBeDefined();
    expect(r.errors.password).toBe("Password is required");
  });
  it("validateField returns null for a valid field", () => {
    expect(validateField("email", "a@b.co")).toBeNull();
  });
  it("validateField returns the first error for an invalid field", () => {
    expect(validateField("email", "bad")).toBe(
      "Please enter a valid email address"
    );
  });
  it("validateField reports unknown fields", () => {
    expect(validateField("nope", "x")).toBe("Field not found in schema");
  });
});

describe("schemas export", () => {
  it("exposes every named schema under a stable key", () => {
    expect(Object.keys(schemas).sort()).toEqual(
      [
        "amount",
        "bid",
        "email",
        "fileUpload",
        "invoice",
        "login",
        "password",
        "percentage",
        "phone",
        "registration",
        "url",
        "username",
        "userProfile",
      ].sort()
    );
  });
});
