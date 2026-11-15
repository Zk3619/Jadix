# @NAME@: OHOS native application

This standalone Cargo workspace mounts the shared Rust form into a UIAbility's ArkUI NodeContent. The generated bundle ID is `@APPLICATION_ID@`; `libjadix_native_controls.so` retains the shared NAPI ABI name.

Use the source checkout's `scripts/native.py build --platform ohos --project <this directory> --sdk <explicit SDK> --architecture x86_64 --offline --output <new report.json>` to build an unsigned API22 HAP. Add `--native-only` for an ELF-only build, including API20. The build freezes this application, framework inputs, lockfiles and the ArkUI project; inspect/package retain those identities.

Installation, launch, actual native events, IME, lifecycle and accessibility require a compatible target and separate evidence. The unsigned HAP does not establish signing or release acceptance. Runtime and persistence acceptance are not inferred from a successful build.

The ArkUI page provides Inspect, Refresh and an explicit Show text switch. Its root-checked nativeInspect NAPI entry returns model nodes, layout, dirty changes, resource budgets and Rust host queue/control/overlay bookkeeping. Inspection does not consume native events. Ordinary text is hidden initially; password values and placeholders stay hidden after opt-in. Closing the panel resets the opt-in, and event polling does not log business state. This diagnostic panel neither queries the platform accessibility tree nor establishes actual visibility or assistive technology behavior. Artifact inspect separately checks retained build and package identities.
