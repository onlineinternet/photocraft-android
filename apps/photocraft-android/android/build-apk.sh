#!/usr/bin/env bash
# Build PhotoCraft's Android APK (arm64-v8a) without Gradle: cargo builds the native library,
# then the SDK's own tools package and sign it (aapt2, zipalign, apksigner).
#
# Needs: Rust with the aarch64-linux-android target, the Android NDK, and the SDK's build-tools
# and a platform. Used by .github/workflows/android.yml; works the same on a Linux machine:
#
#   ANDROID_HOME=~/Android/Sdk ANDROID_NDK_HOME=~/Android/Sdk/ndk/27.2.12479018 \
#     apps/photocraft-android/android/build-apk.sh
#
# Optional: KEYSTORE / KEYSTORE_PASS (signing key; default: a debug key kept in target/android so
# later builds install over earlier ones), VERSION_CODE (must grow for updates), PROFILE
# (release), FEATURES (e.g. heif), MIN_SDK (28), TARGET_SDK (35), BUILD_TOOLS (newest installed).
# Output: target/android/photocraft-android-arm64.apk
set -euo pipefail

die() { echo "build-apk: $*" >&2; exit 1; }

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)

: "${ANDROID_HOME:?set ANDROID_HOME to your Android SDK}"
NDK=${ANDROID_NDK_HOME:-${ANDROID_NDK_LATEST_HOME:-}}
[ -n "$NDK" ] && [ -d "$NDK" ] || die "set ANDROID_NDK_HOME to an installed NDK"

TARGET=aarch64-linux-android
MIN_SDK=${MIN_SDK:-28}
TARGET_SDK=${TARGET_SDK:-35}
PROFILE=${PROFILE:-release}
VERSION_CODE=${VERSION_CODE:-1}
VERSION_NAME=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n1)
BUILD_TOOLS=${BUILD_TOOLS:-$(ls -1 "$ANDROID_HOME/build-tools" 2>/dev/null | sort -V | tail -n1)}
BT="$ANDROID_HOME/build-tools/$BUILD_TOOLS"
JAR="$ANDROID_HOME/platforms/android-$TARGET_SDK/android.jar"
TC="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
[ -x "$BT/aapt2" ] || die "no build-tools in $ANDROID_HOME/build-tools (install e.g. build-tools;35.0.0)"
[ -f "$JAR" ] || die "no $JAR (install platforms;android-$TARGET_SDK)"
[ -x "$TC/clang" ] || die "no NDK toolchain at $TC"

echo "==> Building libphotocraft_android.so ($PROFILE, $TARGET, API $MIN_SDK)"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TC/${TARGET}${MIN_SDK}-clang"
export CC_aarch64_linux_android="$TC/${TARGET}${MIN_SDK}-clang"
export CXX_aarch64_linux_android="$TC/${TARGET}${MIN_SDK}-clang++"
export AR_aarch64_linux_android="$TC/llvm-ar"
# Android 15+ devices may use 16 KB memory pages: align the library's segments for them too.
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="${CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS:-} -C link-arg=-Wl,-z,max-page-size=16384"
features=()
[ -n "${FEATURES:-}" ] && features=(--features "$FEATURES")
(cd "$root" && cargo build --profile "$PROFILE" -p photocraft-android --target "$TARGET" "${features[@]}")

profile_dir=$PROFILE
[ "$PROFILE" = dev ] && profile_dir=debug
so="$root/target/$TARGET/$profile_dir/libphotocraft_android.so"
[ -f "$so" ] || die "the build made no $so"

out="$root/target/android"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out" "$work/lib/arm64-v8a"

echo "==> Packaging"
"$TC/llvm-strip" --strip-unneeded -o "$work/lib/arm64-v8a/libphotocraft_android.so" "$so"
"$BT/aapt2" link -o "$work/unsigned.apk" -I "$JAR" --manifest "$here/AndroidManifest.xml" \
  --min-sdk-version "$MIN_SDK" --target-sdk-version "$TARGET_SDK" \
  --version-code "$VERSION_CODE" --version-name "$VERSION_NAME"
(cd "$work" && zip -q -r unsigned.apk lib)
"$BT/zipalign" -f 4 "$work/unsigned.apk" "$work/aligned.apk"

KEYSTORE_PASS=${KEYSTORE_PASS:-android}
if [ -z "${KEYSTORE:-}" ]; then
  KEYSTORE="$out/debug.keystore"
  if [ ! -f "$KEYSTORE" ]; then
    echo "==> Creating a debug signing key ($KEYSTORE)"
    keytool -genkeypair -noprompt -keystore "$KEYSTORE" -storepass "$KEYSTORE_PASS" -keypass "$KEYSTORE_PASS" \
      -alias photocraft -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=PhotoCraft Android (unofficial)" >/dev/null
  fi
fi

echo "==> Signing"
apk="$out/photocraft-android-arm64.apk"
"$BT/apksigner" sign --ks "$KEYSTORE" --ks-pass "pass:$KEYSTORE_PASS" --key-pass "pass:$KEYSTORE_PASS" --out "$apk" "$work/aligned.apk"
"$BT/apksigner" verify "$apk"
echo "Done: $apk ($(du -h "$apk" | cut -f1))"
