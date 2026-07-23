import { auditService } from "./auditService";

function printUsage() {
  console.log("Usage: npm run verify-audit -- <date>");
  console.log("Example: npm run verify-audit -- 2026-04-25");
  console.log("Verifies the integrity of the audit log for the given YYYY-MM-DD date.");
}

async function main() {
  const dateArg = process.argv[2];

  if (!dateArg || !/^\d{4}-\d{2}-\d{2}$/.test(dateArg)) {
    console.error("Error: Invalid or missing date argument.\n");
    printUsage();
    process.exit(1);
  }

  console.log(`Verifying audit chain for ${dateArg}...`);

  const result = auditService.verifyChain(dateArg);

  if (result.ok) {
    console.log(`✅ Success: Audit chain for ${dateArg} is valid.`);
    process.exit(0);
  } else {
    console.error(`❌ Failure: Audit chain for ${dateArg} is broken at line ${result.brokenAt}.`);
    process.exit(1);
  }
}

main().catch(console.error);