# Mach-O fixtures

These small dylibs export `uv_macho_fixture`, which returns 42. They contain no third-party code.
Regenerate on macOS using the checked-in C source and entitlements:

```sh
cd crates/uv-macho/tests/fixtures
xcrun clang -dynamiclib -arch arm64 -mmacosx-version-min=11.0 -Wl,-install_name,@rpath/libfixture.dylib -Wl,-headerpad,100 -Wl,-no_adhoc_codesign -Wl,-sectcreate,__TEXT,__info_plist,info.plist dylib.c -o arm64.dylib
xcrun clang -dynamiclib -arch x86_64 -mmacosx-version-min=10.9 -Wl,-install_name,@rpath/libfixture.dylib -Wl,-headerpad,100 -Wl,-no_adhoc_codesign dylib.c -o x86_64.dylib
cp arm64.dylib signed-arm64.dylib
codesign --force --sign - --identifier org.astral.uv.fixture --options runtime --force-library-entitlements --entitlements entitlements.plist --requirements '=designated => identifier "org.astral.uv.fixture"' signed-arm64.dylib
```

Compiler and SDK versions may change the binary layout. The tests inspect load commands instead of
depending on fixed offsets, except when testing malformed headers. The unsigned Intel fixture
targets macOS 10.9 to exercise signatures with both SHA-1 and SHA-256 CodeDirectories. The signed
ARM64 fixture contains requirements, XML and DER entitlements, and hardened-runtime metadata. Both ARM64 fixtures
contain an embedded Info.plist.
