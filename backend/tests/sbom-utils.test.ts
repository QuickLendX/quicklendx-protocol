const { validateSbomDocument } = require("../scripts/lib/sbom-utils");

const validSbom = () => ({
  bomFormat: "CycloneDX",
  specVersion: "1.5",
  metadata: {
    component: {
      type: "application",
      name: "quicklendx-backend",
    },
  },
  components: [{ type: "library", name: "express" }],
});

describe("validateSbomDocument", () => {
  it("accepts a structurally valid document without modifying it", () => {
    const document = validSbom();
    const original = JSON.parse(JSON.stringify(document));

    expect(validateSbomDocument(document)).toEqual([]);
    expect(document).toEqual(original);
  });

  it("returns stable, independent errors when validation is retried", () => {
    const document = validSbom();
    document.components.push({ type: "library" });

    const firstResult = validateSbomDocument(document);
    firstResult.push("caller mutation must not affect retries");

    expect(validateSbomDocument(document)).toEqual([
      "components[1] must include type and name",
    ]);
  });

  it.each([
    [
      "a null document",
      null,
      [
        "bomFormat must be CycloneDX",
        "specVersion must be a non-empty string",
        "metadata section is required",
        "metadata.component section is required",
        "components must be an array",
      ],
    ],
    [
      "an array document",
      [],
      [
        "bomFormat must be CycloneDX",
        "specVersion must be a non-empty string",
        "metadata section is required",
        "metadata.component section is required",
        "components must be an array",
      ],
    ],
    [
      "whitespace-only required strings",
      {
        ...validSbom(),
        specVersion: "  ",
        metadata: { component: { type: "\t", name: " " } },
      },
      [
        "specVersion must be a non-empty string",
        "metadata.component.type is required",
        "metadata.component.name is required",
      ],
    ],
    [
      "array values in object-only sections",
      {
        ...validSbom(),
        metadata: [],
      },
      [
        "metadata section is required",
        "metadata.component section is required",
      ],
    ],
    [
      "an invalid first component boundary",
      {
        ...validSbom(),
        components: [[], { type: "library" }],
      },
      ["components[0] must include type and name"],
    ],
  ])(
    "rejects %s deterministically",
    (_description, document, expectedErrors) => {
      expect(validateSbomDocument(document)).toEqual(expectedErrors);
    },
  );
});
