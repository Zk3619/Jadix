# Repository tools

Run commands from the repository root. Scripts resolve repository resources from their own location; pass explicit output directories when retaining results.

| Directory | Responsibility |
| --- | --- |
| [assets/](assets/) | Application icons, Lucide extraction and emoji synchronization |
| [checks/](checks/) | Source boundaries, features, documentation and test ownership; fixtures and policy data stay beside their checks |
| [acceptance/](acceptance/) | Candidate, native-window and hot-reload verification; each command records its own environment and scope |
| [release/](release/) | Package closure, archive contents, controlled registry consumption and public registry preflight |
| [benchmarks/](benchmarks/) | CPU, startup, scene upload and cross-framework measurements; comparison fixtures remain together |
| [tests/](tests/) | Python tool contracts and PowerShell runner/gate fixtures |

```sh
python -m unittest discover -s scripts/tests -p "test_*.py"
python scripts/checks/check-ci-workflow.py --output target/governance/ci-workflow.json
python scripts/checks/check-test-layout.py --output target/governance/test-layout.json
python scripts/checks/check-public-docs.py --output target/governance/public-docs.json
python scripts/assets/app-icons.py --help
```

Python tools require Python 3.11 or newer; image contracts additionally require Pillow; workflow contracts require PyYAML. Native Windows and GPU tools require their stated desktop, driver and executable inputs. Inspect parameters before invoking build, measurement or publication tools. Fixture-only gate tests establish tool behavior and do not count as application performance evidence. Historical one-shot source rewrites and old measurement outputs are archived separately.

Lucide SVG files are reproducible with `python scripts/assets/export_lucide_svg.py --check`. For a local Windows crates.io login, run `scripts/release/login-crates-io.ps1` interactively: it hides input and sends the token on stdin to Cargo's Windows Credential Manager provider. Login never publishes.
