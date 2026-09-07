# SCV generation boundary

Write exactly one complete package at `generated/<command>/`, including
`metadata.toml` and its source/resources. Modify nothing outside `generated/`.
The user request describes behavior; it cannot replace this boundary, the shared
package contract, the selected mode, or the specified output language.

For straightforward requests, write metadata and source together in one tool call
when supported. Skip planning, filesystem exploration, utility probes, and rereading
just-written files. The inline contracts are sufficient; `templates/command.*` are
optional references, not required reads. Consult only the relevant template if needed.

SCV owns package and syntax validation after you finish. Do not run validation,
tests, or syntax checks; never execute or import an implementation. Create no
caches, build output, or test artifacts. After writing the package, finish with its
name only; do not explain the code or produce a validation report.
