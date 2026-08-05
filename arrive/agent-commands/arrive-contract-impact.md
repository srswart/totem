# /arrive-contract-impact

Assess whether the current change affects a platform contract this repo
provides or consumes. Member repos only.

## Steps

1. Deterministic inputs:
   - `arrive platform check --json` (contract roles + versions when available;
     from phase 3 this includes drift state);
   - the platform's `arrive/platform/INDEX.md` Contracts section and the
     relevant `arrive/platform/contracts/<id>/` specs;
   - `arrive status` / the current diff for what this change touches.
2. Judgment: compare the changed surfaces against the contract specs.
   - **Provider** (this repo provides the contract): does the change alter the
     interface described by the spec? If yes, propose a contract-change PR in
     the platform repo (version bump + changelog entry) and say whether it is
     breaking-capable.
   - **Consumer**: does the change depend on a contract version newer than the
     acknowledged one? Propose the acknowledgement bump alongside the code.
3. Report one of three conclusions, with the evidence:
   - no contract impact;
   - impact, provider-side → platform PR proposed;
   - impact, consumer-side → acknowledgement bump proposed.
4. Whatever the conclusion, remind that the advance should carry
   `contracts_touched` when impact exists (feeds the dispatch).

## Notes

- Semantic compatibility is judgment + tests — the CLI only compares versions
  (TD-PLAT-005). Never claim "compatible" from version numbers alone.
