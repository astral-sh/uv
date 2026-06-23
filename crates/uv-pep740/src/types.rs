//! PEP 740 attestation support types.

use std::collections::HashMap;

use base64::Engine as _;
use serde::{Deserialize as _, Serialize as _};
use sigstore_types::Sha256Hash;

/// A to-be-signed Python distribution.
#[derive(Debug)]
pub struct TBSDistribution {
    pub(crate) filename: uv_distribution_filename::DistFilename,
    pub(crate) digest: Sha256Hash,
}

impl TBSDistribution {
    pub fn new(filename: uv_distribution_filename::DistFilename, digest: &str) -> Option<Self> {
        Some(Self {
            filename,
            digest: Sha256Hash::from_hex(digest).ok()?,
        })
    }
}

/// A single PEP 740 attestation.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Attestation {
    /// The attestation's version, which is always 1.
    #[serde(deserialize_with = "require_version_one")]
    pub version: usize,
    /// The attestation's verification material.
    pub verification_material: VerificationMaterial,
    /// The attestation's envelope, i.e. statement and signature.
    pub envelope: Envelope,
}

fn require_version_one<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let version = usize::deserialize(deserializer)?;
    if version == 1 {
        Ok(version)
    } else {
        Err(serde::de::Error::custom(format!(
            "unsupported attestation version: {version}"
        )))
    }
}

impl TryFrom<sigstore_types::Bundle> for Attestation {
    type Error = &'static str;

    fn try_from(value: sigstore_types::Bundle) -> Result<Self, Self::Error> {
        Ok(Self {
            version: 1,
            verification_material: value.verification_material.try_into()?,
            envelope: value.content.try_into()?,
        })
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VerificationMaterial {
    /// The attestation's certificate, as DER.
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub certificate: Vec<u8>,

    /// One or more transparency log entries for the attestation.
    pub transparency_entries: TransparencyLogEntries,
}

impl TryFrom<sigstore_types::VerificationMaterial> for VerificationMaterial {
    type Error = &'static str;

    fn try_from(value: sigstore_types::VerificationMaterial) -> Result<Self, Self::Error> {
        let certificate = match value.content {
            sigstore_types::bundle::VerificationMaterialContent::Certificate(cert) => {
                cert.raw_bytes.into_bytes()
            }
            // TODO: Should we bother supporting this?
            // Our signing flow only generates v3+ bundles, which only use the
            // single certificate format. In principle this is easy to support
            // by pulling the first member of the chain (which is required to be the
            // leaf), but in practice we'll need to be more defensive since producers
            // might use the wrong chain order.
            sigstore_types::bundle::VerificationMaterialContent::X509CertificateChain {
                ..
            } => return Err("certificate chains are not supported (expected a v3 bundle)"),
            sigstore_types::bundle::VerificationMaterialContent::PublicKey { .. } => {
                return Err("expected a certificate, not a public key");
            }
        };

        // PEP 740 doesn't enforce a specific transparency log entry layout, so
        // our conversion here is intentionally loose. We intentionally round-trip
        // through JSON to avoid having to strictly define our expected fields.
        let transparency_entries = value
            .tlog_entries
            .into_iter()
            .map(|entry| serde_json::to_string(&entry).and_then(|s| serde_json::from_str(&s)))
            .collect::<Result<Vec<TransparencyLogEntry>, _>>()
            .map_err(|_| "failed to convert transparency log entry")?;

        let Some(transparency_entries) = TransparencyLogEntries::new(transparency_entries) else {
            return Err("at least one transparency log entry is required");
        };

        Ok(Self {
            certificate,
            transparency_entries,
        })
    }
}

type TransparencyLogEntry = HashMap<String, serde_json::Value>;

#[derive(Debug, serde::Serialize)]
pub struct TransparencyLogEntries(pub Vec<TransparencyLogEntry>);

impl TransparencyLogEntries {
    pub(crate) fn new(entries: Vec<TransparencyLogEntry>) -> Option<Self> {
        if entries.is_empty() {
            None
        } else {
            Some(Self(entries))
        }
    }
}

impl<'de> serde::Deserialize<'de> for TransparencyLogEntries {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let entries = Vec::deserialize(deserializer)?;
        Self::new(entries)
            .ok_or_else(|| serde::de::Error::custom("transparency log entries cannot be empty"))
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Envelope {
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub statement: Vec<u8>,
    #[serde(serialize_with = "to_base64", deserialize_with = "from_base64")]
    pub signature: Vec<u8>,
}

impl TryFrom<sigstore_types::SignatureContent> for Envelope {
    type Error = &'static str;

    fn try_from(value: sigstore_types::SignatureContent) -> Result<Self, Self::Error> {
        let sigstore_types::SignatureContent::DsseEnvelope(dsse) = value else {
            return Err("expected a DSSE envelope");
        };

        let [signature] = dsse.signatures.as_slice() else {
            return Err("expected exactly one signature in the DSSE envelope");
        };

        Ok(Self {
            statement: dsse.payload.into_bytes(),
            signature: signature.sig.clone().into_bytes(),
        })
    }
}

fn to_base64<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let s = base64::engine::general_purpose::STANDARD.encode(bytes);
    s.serialize(serializer)
}

fn from_base64<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| serde::de::Error::custom(format!("invalid base64: {e}")))
}

#[cfg(test)]
mod tests {
    use crate::types::Attestation;

    #[test]
    fn test_deserialize_attestation() {
        // Extracted from <https://pypi.org/integrity/juwunit/0.1.3/juwunit-0.1.3-cp310-abi3-macosx_10_12_x86_64.whl/provenance>
        let json = r#"
        {
          "envelope": {
            "signature": "MEYCIQCyXTJJn5vb2q6NOjwuF2bIFwK3/ozzJ8mJlwm4WyYQBwIhAJG47JB/HO56JLl9drAESVdxM4WTFnPLehUiDkBDkcgv",
            "statement": "eyJfdHlwZSI6Imh0dHBzOi8vaW4tdG90by5pby9TdGF0ZW1lbnQvdjEiLCJzdWJqZWN0IjpbeyJuYW1lIjoianV3dW5pdC0wLjEuMy1jcDMxMC1hYmkzLW1hY29zeF8xMF8xMl94ODZfNjQud2hsIiwiZGlnZXN0Ijp7InNoYTI1NiI6IjJmNmFiNGFlOThhZDgyOWY5ODEwMGQ5ODg0Y2YxMDFmNGU3Njg1ZjY4ZGE2OGFkMDIwYmNjYTg2ODVmZGMxMzAifX1dLCJwcmVkaWNhdGVUeXBlIjoiaHR0cHM6Ly9kb2NzLnB5cGkub3JnL2F0dGVzdGF0aW9ucy9wdWJsaXNoL3YxIiwicHJlZGljYXRlIjpudWxsfQ=="
          },
          "verification_material": {
            "certificate": "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",
            "transparency_entries": [
              {
                "canonicalizedBody": "eyJhcGlWZXJzaW9uIjoiMC4wLjEiLCJraW5kIjoiZHNzZSIsInNwZWMiOnsiZW52ZWxvcGVIYXNoIjp7ImFsZ29yaXRobSI6InNoYTI1NiIsInZhbHVlIjoiNTdhZDQyODhlYmRmOWRhNGNlMThkMGQ1OGZiYzBjMjI4Njc3ODNiYWEwYzNhNTg3ZTZkZjI5NWNlZDU0MzJmOSJ9LCJwYXlsb2FkSGFzaCI6eyJhbGdvcml0aG0iOiJzaGEyNTYiLCJ2YWx1ZSI6IjljZDlmNjc4ZGM5YWRlYWYxZmYyZjM0N2I1NjdhNDNlMjg4MjAzNTMyN2EwY2I4NTkzN2Y0Y2VlZTg0MWQwN2MifSwic2lnbmF0dXJlcyI6W3sic2lnbmF0dXJlIjoiTUVZQ0lRQ3lYVEpKbjV2YjJxNk5Pand1RjJiSUZ3SzMvb3p6SjhtSmx3bTRXeVlRQndJaEFKRzQ3SkIvSE81NkpMbDlkckFFU1ZkeE00V1RGblBMZWhVaURrQkRrY2d2IiwidmVyaWZpZXIiOiJMUzB0TFMxQ1JVZEpUaUJEUlZKVVNVWkpRMEZVUlMwdExTMHRDazFKU1VkM1ZFTkRRbXRwWjBGM1NVSkJaMGxWWmxoR055dFhZMWxyVEhnNFJrSTFOVVJ2T0ZwTVRHbEdjMVpGZDBObldVbExiMXBKZW1vd1JVRjNUWGNLVG5wRlZrMUNUVWRCTVZWRlEyaE5UV015Ykc1ak0xSjJZMjFWZFZwSFZqSk5ValIzU0VGWlJGWlJVVVJGZUZaNllWZGtlbVJIT1hsYVV6RndZbTVTYkFwamJURnNXa2RzYUdSSFZYZElhR05PVFdwWmQwNVVSWHBOYWtFeFRrUkJORmRvWTA1TmFsbDNUbFJGZWsxcVJYZE9SRUUwVjJwQlFVMUdhM2RGZDFsSUNrdHZXa2w2YWpCRFFWRlpTVXR2V2tsNmFqQkVRVkZqUkZGblFVVjBha29yVURKdFZrSk9PVFpwVW1welRWaHVhMFpqTlVOd1NGRTNMM2szUTA1cFUyb0tlR3RqZFhac2VFRXliWFEyUVZaT2FUUk1lRWRqU0VoU1IwRllMMGRNV0RCcWVpdFpUVUYxWVdaRWRISnVObUo0WkRaUFEwSlhZM2RuWjFacVRVRTBSd3BCTVZWa1JIZEZRaTkzVVVWQmQwbElaMFJCVkVKblRsWklVMVZGUkVSQlMwSm5aM0pDWjBWR1FsRmpSRUY2UVdSQ1owNVdTRkUwUlVablVWVnBjMHM1Q2s5T1RHcDZZVUpFV21KVU1HUm1VVGwxU0haWlVHeHJkMGgzV1VSV1VqQnFRa0puZDBadlFWVXpPVkJ3ZWpGWmEwVmFZalZ4VG1wd1MwWlhhWGhwTkZrS1drUTRkMWxSV1VSV1VqQlNRVkZJTDBKR1kzZFdXVnBVWVVoU01HTklUVFpNZVRsdVlWaFNiMlJYU1hWWk1qbDBUREpHZW1SSVNtaGlRekY2WVVNNWNRcGtXR1F4WW0xc01FeDVOVzVoV0ZKdlpGZEpkbVF5T1hsaE1scHpZak5rZWt3elFqRlpiWGh3WXpKbmRXVlhNWE5SU0Vwc1dtNU5kbVJIUm01amVUa3lDazFETkhoTWFrMTNUMUZaUzB0M1dVSkNRVWRFZG5wQlFrRlJVWEpoU0ZJd1kwaE5Oa3g1T1RCaU1uUnNZbWsxYUZrelVuQmlNalY2VEcxa2NHUkhhREVLV1c1V2VscFlTbXBpTWpVd1dsYzFNRXh0VG5aaVZFRlRRbWR2Y2tKblJVVkJXVTh2VFVGRlEwSkJVbmRrV0U1dlRVUlpSME5wYzBkQlVWRkNaemM0ZHdwQlVVMUZTMFJTYlU1NmFHMWFWR1JyV2xkUk0wMUhTVFZOUjFaclRVUlZORnBxUlhwWlZHczBXVlJzYWxsdFVtcFpWRUpxV2xkTk1FNTZaM2RHVVZsTENrdDNXVUpDUVVkRWRucEJRa0pCVVVoVlNGWnBZa2RzZW1GRVFXWkNaMjl5UW1kRlJVRlpUeTlOUVVWR1FrSkdhR016VW5sWlYzZDBZekpuZG1GdVZqTUtaRmMxY0dSRVFXVkNaMjl5UW1kRlJVRlpUeTlOUVVWSFFrSkNlVnBYV25wTU0xSm9Xak5OZG1ScVFYVk5VelI2VFVSelIwTnBjMGRCVVZGQ1p6YzRkd3BCVVdkRlRGRjNjbUZJVWpCalNFMDJUSGs1TUdJeWRHeGlhVFZvV1ROU2NHSXlOWHBNYldSd1pFZG9NVmx1Vm5wYVdFcHFZakkxTUZwWE5UQk1iVTUyQ21KVVFtcENaMjl5UW1kRlJVRlpUeTlOUVVWS1FrWlZUVlV5YURCa1NFSjZUMms0ZGxveWJEQmhTRlpwVEcxT2RtSlRPV2hqTTFKNVdWZDNkR015WjNZS1lXNVdNMlJYTlhCa1F6aDFXakpzTUdGSVZtbE1NMlIyWTIxMGJXSkhPVE5qZVRsM1pGZEtjMkZZVG05TWJteDBZa1ZDZVZwWFducE1NMUpvV2pOTmRncGtha0YxVFZNMGVrMUVaMGREYVhOSFFWRlJRbWMzT0hkQlVXOUZTMmQzYjA1SFdUTlBSMXBzVGpKU2JGcEVZM2RaYW10M1dsZFJkMDVVYUcxTlZFNW9DazlVYUdoUFYwNXBXa2RPYUUxSFRteFplbEV6VDBSQlpFSm5iM0pDWjBWRlFWbFBMMDFCUlV4Q1FUaE5SRmRrY0dSSGFERlphVEZ2WWpOT01GcFhVWGNLVGtGWlMwdDNXVUpDUVVkRWRucEJRa1JCVVcxRVExSnZaRWhTZDJONmIzWk1NbVJ3WkVkb01WbHBOV3BpTWpCMldWaE9NR050Um5OTVdFNXZUREp3TVFwa00xWjFZVmhSZDA5QldVdExkMWxDUWtGSFJIWjZRVUpFVVZGeFJFTm5NRnBxWXpSYWJWVXpXa2RXYTA1NlFtbFBWRUpzV2tSQk1VOUhXWGhOTWtVMUNrOUhSVFZaTWtwcldUSkZkMWt5Vm1wT1JHTTBUVU5CUjBOcGMwZEJVVkZDWnpjNGQwRlJORVZGWjNkUlkyMVdiV041T1RCWlYyUjZURE5aZDB4cVJYVUtUWHBCWVVKbmIzSkNaMFZGUVZsUEwwMUJSVkJDUVhkTlEycEZlVTE2VlRSTmVrbDZUMVJSZDB4QldVdExkMWxDUWtGSFJIWjZRVUpGUVZGbFJFSjRid3BrU0ZKM1kzcHZka3d5WkhCa1IyZ3hXV2sxYW1JeU1IWlpXRTR3WTIxR2MweFlUbTlOUW10SFEybHpSMEZSVVVKbk56aDNRVkpGUlVOM2QwcE5WRVV4Q2s5VVdYbFBSRTAxVFVkTlIwTnBjMGRCVVZGQ1p6YzRkMEZTU1VWV1VYaFVZVWhTTUdOSVRUWk1lVGx1WVZoU2IyUlhTWFZaTWpsMFRESkdlbVJJU21nS1lrTXhlbUZET1hGa1dHUXhZbTFzTUV4NU5XNWhXRkp2WkZkSmRtUXlPWGxoTWxwellqTmtla3d6UWpGWmJYaHdZekpuZFdWWE1YTlJTRXBzV201TmRncGtSMFp1WTNrNU1rMUROSGhNYWsxM1QwRlpTMHQzV1VKQ1FVZEVkbnBCUWtWM1VYRkVRMmN3V21wak5GcHRWVE5hUjFaclRucENhVTlVUW14YVJFRXhDazlIV1hoTk1rVTFUMGRGTlZreVNtdFpNa1YzV1RKV2FrNUVZelJOUWxGSFEybHpSMEZSVVVKbk56aDNRVkpSUlVKbmQwVmpTRlo2WVVSQ1dVSm5iM0lLUW1kRlJVRlpUeTlOUVVWV1FrVnZUVk5IYURCa1NFSjZUMms0ZGxveWJEQmhTRlpwVEcxT2RtSlRPV2hqTTFKNVdWZDNkR015WjNaaGJsWXpaRmMxY0Fwa1F6bG9XVE5TY0dJeU5YcE1NMG94WW01TmRrMXFWVFJOYWxVd1RsUlplazE2VlhaWldGSXdXbGN4ZDJSSVRYWk5WRUZZUW1kdmNrSm5SVVZCV1U4dkNrMUJSVmRDUVd0TlFqTkNlV0ZZV21oa1IxVjNSa0ZaUzB0M1dVSkNRVWRFZG5wQlFrWjNVVWRFUVZKM1pWaENjRTFKUjBwQ1oyOXlRbWRGUlVGa1dqVUtRV2RSUTBKSWMwVmxVVUl6UVVoVlFUTlVNSGRoYzJKSVJWUktha2RTTkdOdFYyTXpRWEZLUzFoeWFtVlFTek12YURSd2VXZERPSEEzYnpSQlFVRkhaUXBKZURWTlUwRkJRVUpCVFVGU2FrSkZRV2xDT0hweGRWRXJjMXB6VEZveFpUbDZNa2s0ZWl0TmJ6aGxTQ3RqWkVOb2NtRjNUV05uTURCSVFuTkNaMGxuQ2t3MFF6RnFURFJIYURKSmNFMDRUbEZuUVVWT09VcERXRmRXWmxwWE9HNXVSbVkwYzI1WUszbG9VbmQzUTJkWlNVdHZXa2w2YWpCRlFYZE5SRnAzUVhjS1drRkpkMDR6TUZkUlRHeG9TRm94VFhOdmNEa3JXV1p3VlhCSmFYTjBaVkZVYlZOVVRtZ3hka3RPYms1VWFFRkNZMGdyZW5wTmR6SXpRMVpETmpkclV3cFJibWRHUVdwQ2JYcFplRU5rZHpGYVVVVmhlRGx2Y0hCSmNqaEZSRk5xZWxaWVFWSkRhRGRWUmk5UU5uUmphRXROWmpFeGVFTnRXbkZKYjJWV2RVVXZDak5tVGtacU1EUTlDaTB0TFMwdFJVNUVJRU5GVWxSSlJrbERRVlJGTFMwdExTMEsifV19fQ==",
                "inclusionPromise": {
                  "signedEntryTimestamp": "MEUCIDAQNcheEbBYycMBg0vRtvekKm/xDfYvXOpTd/MLg3UGAiEA/x5jHN1CcUKodTF0LyUnCcmcitwB6MG/DpvEEtdCoDg="
                },
                "inclusionProof": {
                  "checkpoint": {
                    "envelope": "rekor.sigstore.dev - 1193050959916656506\n1404235696\niBf8M0b4ZuNvO87syhTowH6p328D3/AjDKvqtZ2y/iA=\n\n— rekor.sigstore.dev wNI9ajBEAiA6g/eoaBH6Mn5rC8tSi8ZV0dk36z8Um32OAKBzOQhE8QIgUViKHB1sg6OV0YGlfbb4LavLO4R0UDZa7KRvR+CscEQ=\n"
                  },
                  "hashes": [
                    "KCwX6kVSxdqo1GvZu2o8xCdjpjlfxUwjYbmmdyed3q8=",
                    "p6fi/+8D/yYIoRn+b4yl2FFpMkem7wCEFpQ3Zn9N4wM=",
                    "2i/8++0qzmDAyp0An0BrGYszGLxs+EvlIjfM5N17OT4=",
                    "4TSsgQwXNivpG/NX9TZR5XFGILkgAXWx7k0eM67iaNI=",
                    "8X7JD8SW25/RvwDGQAXp1sR7FJoM6ahEbFR7GI40U7k=",
                    "rIgCpr9nLQxTM7P/5yUYIMy2XR2hpEBNkKScWHmqusk=",
                    "6y+mmJ23NeVZBA49evGHBxFNuJzVR7SMsaYBVGb2gJk=",
                    "mVdqxCRcdO61RVWLdbm1E9tFDe0JGAsY/lvsY4tCYU0=",
                    "FO6HN38yxXp6q+nc8YdCcwSewpP/geQwlQ0Lhx1vPf4=",
                    "NK5V4uUWGURac0+ZHSsKAN+G2uWPh0Eyw+q0xTOgYvo=",
                    "64q4HbWbrNqCG1sxH0J2ZGjwieOFvUtfbOP+/mF7IaM=",
                    "5fFgd8abI5slJ3xCel5hsKjMId+SdqNJnAvIkAjiTYk=",
                    "WVLj88wXW0G2xalgu9jhVM1qskn7q53whvS1L8yosKE=",
                    "BxLXJ5RkwS5iTf7X9Mnw2ndRluMbCWMpBMW2xt0OB2U=",
                    "2pgl+K2FBMPqwi/RiMwFIpnklChzSzjpqrWxNvv1CmU=",
                    "vT7m3ljLcmpEJXnhWzajNOUgEtsZqmeXFhKHis8vIvQ=",
                    "wT/TVy00Yzh7P0tw0CphKWRYutXp50AASgtwdEKQVW8=",
                    "2EiO/S9Up9YTXZeEvtg9Fx/ufU6roA4WuaT5LRMWZ6w=",
                    "aEYBXnvG8xvNhFhDlPiZI0VR8Yh9HSmPqzVTB6Hr3uI=",
                    "uSZE5XoKhloeXM8FdFSXuX/GNXpNxFYcwOY0L+3E4qA=",
                    "eT+F471g2HJfd43U4j4L1PIBkt4rLbHQd/pOR/rllO0=",
                    "DOCeoSMovIvLExkhIvisow9AuNXgeWs4ECkyR6EcqYU="
                  ],
                  "logIndex": "1404235670",
                  "rootHash": "iBf8M0b4ZuNvO87syhTowH6p328D3/AjDKvqtZ2y/iA=",
                  "treeSize": "1404235696"
                },
                "integratedTime": "1778705653",
                "kindVersion": {
                  "kind": "dsse",
                  "version": "0.0.1"
                },
                "logId": {
                  "keyId": "wNI9atQGlz+VWfO6LRygH4QUfY/8W4RFwiT5i5WRgB0="
                },
                "logIndex": "1526139932"
              }
            ]
          },
          "version": 1
        }
        "#;

        let attestation = serde_json::from_str::<Attestation>(json);
        assert!(attestation.is_ok());
    }
}
