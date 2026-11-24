# Pull Request Template

## 📝 Description
<!-- Provide a brief description of the changes made in this PR -->

## 🎯 Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update
- [ ] Refactoring
- [ ] Performance improvement
- [ ] Security enhancement
- [ ] Other (please describe):

## 🔧 Changes Made
<!-- List specific changes, files modified, and new functionality added -->

### Files Modified
- 

### New Files Added
- 

### Key Changes
- 

## 🧪 Testing
- [ ] Unit tests pass
- [ ] Integration tests pass
- [ ] Manual testing completed
- [ ] No breaking changes introduced
- [ ] Cross-platform compatibility verified
- [ ] Edge cases tested

### Test Coverage
<!-- Describe what was tested and how -->

## 📋 Contract-Specific Checks
- [ ] Soroban contract builds successfully (`make build` / `make wasm`)
- [ ] Formatting passes (`cargo fmt --all -- --check` / `make fmt-check`)
- [ ] Clippy linter passes with no warnings (`cargo clippy --lib -- -D warnings` / `make lint`)
- [ ] Library tests pass (`cargo test --lib` / `make test-lib`)
- [ ] WASM size check passes (`./scripts/check-wasm-size.sh` / `make size`)
- [ ] No existing error codes reordered or changed (error-code stability preserved)
- [ ] Contract documentation updated (README, `CONTRIBUTING.md`, or inline docs as applicable)
- [ ] Gas usage optimized
- [ ] Security considerations reviewed
- [ ] Events properly emitted
- [ ] Error handling implemented
- [ ] Access control verified

### Contract Testing Details
<!-- Describe any specific contract testing performed -->

## 📋 Review Checklist
- [ ] Code follows project style guidelines
- [ ] Documentation updated if needed
- [ ] No sensitive data exposed
- [ ] Error handling implemented
- [ ] Edge cases considered
- [ ] Code is self-documenting
- [ ] No hardcoded values
- [ ] Proper logging implemented

## 🔍 Code Quality
- [ ] Clippy warnings addressed
- [ ] Code formatting follows rustfmt standards
- [ ] No unused imports or variables
- [ ] Functions are properly documented
- [ ] Complex logic is commented

## 🚀 Performance & Security
- [ ] Gas optimization reviewed
- [ ] No potential security vulnerabilities
- [ ] Input validation implemented
- [ ] Access controls properly configured
- [ ] No sensitive information in logs

## 📚 Documentation
- [ ] README updated if needed
- [ ] Code comments added for complex logic
- [ ] API documentation updated
- [ ] Changelog updated (if applicable)

## 🔗 Related Issues
<!-- Link to any related issues -->
Closes #
Fixes #
Related to #

## 📋 Additional Notes
<!-- Any additional information that reviewers should know -->

## 🧪 How to Test
<!-- Provide step-by-step instructions for testing this PR -->

1. 
2. 
3. 

## 📸 Screenshots (if applicable)
<!-- Add screenshots for UI changes -->

## ⚠️ Breaking Changes
<!-- List any breaking changes and migration steps -->

## 🔄 Migration Steps (if applicable)
<!-- Provide migration steps for breaking changes -->

---

## 📋 Reviewer Checklist
<!-- For reviewers to check off -->

### Code Review
- [ ] Code is readable and well-structured
- [ ] Logic is correct and efficient
- [ ] Error handling is appropriate
- [ ] Security considerations addressed
- [ ] Performance impact assessed

### Contract Review
- [ ] Contract logic is sound
- [ ] Gas usage is reasonable
- [ ] Events are properly emitted
- [ ] Access controls are correct
- [ ] Edge cases are handled

### Documentation Review
- [ ] Code is self-documenting
- [ ] Comments explain complex logic
- [ ] README updates are clear
- [ ] API changes are documented

### Testing Review
- [ ] Tests cover new functionality
- [ ] Tests are meaningful and pass
- [ ] Edge cases are tested
- [ ] Integration tests work correctly