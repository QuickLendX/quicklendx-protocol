# Issue #1063 export implementation

This file previously indexed export code in the retired duplicate backend
package and is no longer an implementation inventory. The canonical export
workflow is documented in [backend/docs/exports.md](backend/docs/exports.md).
Its current route, controller, and service are under `backend/src/`.

Some historical features described in the old index (including its independent
export audit/statistics API and the contract-test harness) were not carried
forward because they are not part of the canonical backend implementation.
