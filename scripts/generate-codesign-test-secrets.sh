#!/usr/bin/env bash
## Generate a self-signed code signing certificate and populate a GitHub
## environment with the resulting secrets and variables via the `gh` CLI.
##
## Secrets: CODESIGN_CERTIFICATE_PASSWORD, CODESIGN_IDENTITY_MACOS,
##   CODESIGN_CERTIFICATE_MACOS, CODESIGN_CERTIFICATE_WINDOWS
## Variables: CODESIGN_ALLOW_UNTRUSTED_MACOS
##
## Usage:
##
##   scripts/generate-codesign-test-secrets.sh

set -euo pipefail

if ! command -v gh &>/dev/null; then
  echo "error: gh CLI is required but not found. Install from https://cli.github.com" >&2
  exit 1
fi

REPO="astral-sh/uv"
ENV_NAME="release-test"

echo "Generating self-signed code signing certificate..."

CERT_DIR="$(mktemp -d)"
trap 'rm -rf "$CERT_DIR"' EXIT

CERT_NAME="uv-codesign-test"
P12_PASSWORD="$(uuidgen | tr -d '-')"

# ---------------------------------------------------------------------------
# Generate a self-signed code-signing certificate as a PKCS#12 / PFX.
# The same file is used for both macOS (.p12) and Windows (.pfx) — they are
# the same format.
# ---------------------------------------------------------------------------

openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
  -keyout "$CERT_DIR/key.pem" \
  -out "$CERT_DIR/cert.pem" \
  -subj "/CN=$CERT_NAME" \
  -addext "extendedKeyUsage=codeSigning" \
  -addext "keyUsage=digitalSignature" \
  2>/dev/null

# Detect whether we need -legacy (OpenSSL 3.x requires it for macOS keychain
# compatibility; LibreSSL shipped with macOS does not support it).
LEGACY_FLAG=""
if openssl version 2>&1 | grep -q "^OpenSSL 3"; then
  LEGACY_FLAG="-legacy"
fi

# shellcheck disable=SC2086
openssl pkcs12 -export $LEGACY_FLAG \
  -inkey "$CERT_DIR/key.pem" \
  -in "$CERT_DIR/cert.pem" \
  -name "$CERT_NAME" \
  -out "$CERT_DIR/cert.p12" \
  -passout pass:"$P12_PASSWORD" \
  2>/dev/null

CERT_B64="$(base64 < "$CERT_DIR/cert.p12" | tr -d '\n')"
CERT_SHA1="$(openssl x509 -in "$CERT_DIR/cert.pem" -noout -fingerprint -sha1 \
  | cut -d= -f2 | tr -d ':')"

# ---------------------------------------------------------------------------
# Populate the GitHub environment.
# ---------------------------------------------------------------------------

echo "Setting secrets and variables in '${ENV_NAME}' environment for ${REPO}..."

gh secret set CODESIGN_CERTIFICATE_PASSWORD \
  --repo "$REPO" --env "$ENV_NAME" --body "$P12_PASSWORD"

gh secret set CODESIGN_IDENTITY_MACOS \
  --repo "$REPO" --env "$ENV_NAME" --body "$CERT_SHA1"

gh secret set CODESIGN_CERTIFICATE_MACOS \
  --repo "$REPO" --env "$ENV_NAME" --body "$CERT_B64"

gh secret set CODESIGN_CERTIFICATE_WINDOWS \
  --repo "$REPO" --env "$ENV_NAME" --body "$CERT_B64"

gh variable set CODESIGN_ALLOW_UNTRUSTED_MACOS \
  --repo "$REPO" --env "$ENV_NAME" --body "1"

echo ""
echo "Done. Set in '${ENV_NAME}' environment for ${REPO}:"
echo "  CODESIGN_CERTIFICATE_PASSWORD"
echo "  CODESIGN_IDENTITY_MACOS"
echo "  CODESIGN_CERTIFICATE_MACOS"
echo "  CODESIGN_CERTIFICATE_WINDOWS"
echo "  CODESIGN_ALLOW_UNTRUSTED_MACOS"
