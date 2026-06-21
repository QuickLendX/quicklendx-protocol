#!/usr/bin/env node

/**
 * Simple validation script to verify database optimization changes
 * without running the full test suite (which requires npm install).
 */

const fs = require('fs');
const path = require('path');

console.log('🔍 Validating Database Optimization Changes...\n');

const checks = [
  {
    name: 'Database pragmas updated',
    file: 'src/lib/database.ts',
    tests: [
      { pattern: /pragma\('synchronous = NORMAL'\)/, desc: 'synchronous = NORMAL pragma' },
      { pattern: /pragma\('journal_mode = WAL'\)/, desc: 'journal_mode = WAL pragma' },
      { pattern: /pragma\('busy_timeout = 5000'\)/, desc: 'busy_timeout pragma' },
    ]
  },
  {
    name: 'Statement cache implemented',
    file: 'src/lib/database.ts',
    tests: [
      { pattern: /getPreparedStatement/, desc: 'getPreparedStatement function' },
      { pattern: /statementCache = new Map/, desc: 'statement cache Map' },
      { pattern: /clearStatementCache/, desc: 'clearStatementCache function' },
      { pattern: /getStatementCacheStats/, desc: 'getStatementCacheStats function' },
    ]
  },
  {
    name: 'InvoiceStore uses cached statements',
    file: 'src/services/invoiceStore.ts',
    tests: [
      { pattern: /getPreparedStatement/, desc: 'imports getPreparedStatement' },
      { pattern: /getPreparedStatement\(['"]/g, desc: 'uses getPreparedStatement (multiple times)', count: 2 },
    ]
  },
  {
    name: 'API Key DB uses cached statements',
    file: 'src/db/database.ts',
    tests: [
      { pattern: /getPreparedStatement/, desc: 'imports and uses getPreparedStatement' },
      { pattern: /getPreparedStatement\(['"]/g, desc: 'uses getPreparedStatement (multiple times)', count: 8 },
    ]
  },
  {
    name: 'NotificationService uses cached statements',
    file: 'src/services/notificationService.ts',
    tests: [
      { pattern: /getPreparedStatement/, desc: 'imports getPreparedStatement' },
    ]
  },
  {
    name: 'Performance tests added',
    file: 'src/tests/perf/perf.test.ts',
    tests: [
      { pattern: /Statement Cache Performance/, desc: 'statement cache performance test' },
      { pattern: /WAL mode is enabled/, desc: 'WAL mode verification test' },
      { pattern: /synchronous mode is NORMAL/, desc: 'synchronous pragma verification' },
      { pattern: /busy_timeout is configured/, desc: 'busy_timeout verification' },
    ]
  },
  {
    name: 'Documentation updated',
    file: '../docs/persistence.md',
    tests: [
      { pattern: /Prepared Statement Cache/, desc: 'statement cache documentation' },
      { pattern: /synchronous = NORMAL/, desc: 'synchronous pragma documentation' },
      { pattern: /journal_mode = WAL/, desc: 'WAL mode documentation' },
      { pattern: /Performance Benchmarks/, desc: 'performance benchmarks section' },
    ]
  }
];

let allPassed = true;
let totalTests = 0;
let passedTests = 0;

checks.forEach(check => {
  const filePath = path.join(__dirname, check.file);
  
  if (!fs.existsSync(filePath)) {
    console.log(`❌ ${check.name}: File not found: ${check.file}`);
    allPassed = false;
    return;
  }
  
  const content = fs.readFileSync(filePath, 'utf-8');
  let checkPassed = true;
  
  check.tests.forEach(test => {
    totalTests++;
    if (test.count) {
      const matches = content.match(test.pattern);
      const actualCount = matches ? matches.length : 0;
      if (actualCount >= test.count) {
        passedTests++;
        console.log(`  ✅ ${test.desc}: found ${actualCount} occurrences`);
      } else {
        console.log(`  ❌ ${test.desc}: expected at least ${test.count}, found ${actualCount}`);
        checkPassed = false;
      }
    } else {
      if (test.pattern.test(content)) {
        passedTests++;
        console.log(`  ✅ ${test.desc}`);
      } else {
        console.log(`  ❌ ${test.desc}: not found`);
        checkPassed = false;
      }
    }
  });
  
  if (checkPassed) {
    console.log(`✅ ${check.name}\n`);
  } else {
    console.log(`❌ ${check.name}\n`);
    allPassed = false;
  }
});

console.log(`\n${'='.repeat(60)}`);
console.log(`📊 Validation Summary: ${passedTests}/${totalTests} checks passed`);
console.log('='.repeat(60));

if (allPassed) {
  console.log('\n✅ All validations passed! Changes look good.');
  console.log('\n📝 Next steps:');
  console.log('   1. Run: cd backend && npm install');
  console.log('   2. Run: npm test -- perf.test.ts');
  console.log('   3. Run: npm test (full test suite)');
  process.exit(0);
} else {
  console.log('\n❌ Some validations failed. Please review the changes.');
  process.exit(1);
}
