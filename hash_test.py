from hashlib import blake2b
from os import fspath
from typing import Optional, Union

Pathish = Union[str, bytes, "os.PathLike[str]", "os.PathLike[bytes]"]

def git_cache_digest(repository: str, precise: str, subdirectory: Optional[Pathish] = None) -> str:
    """
    Reproduces the Rust digest() exactly:

    - blake2b with 32-byte (256-bit) digest
    - bytes fed in this order:
        repository + "/" + precise [+ "?subdirectory=" + subdirectory]
    - subdirectory is included only if it is representable as UTF-8
      (mirrors Rust Path::to_str() -> Option<&str>)
    - hex output is lowercase
    """
    h = blake2b(digest_size=32)

    # repository and precise are Rust &str equivalents: encode as UTF-8
    h.update(repository.encode("utf-8"))
    h.update(b"/")
    h.update(precise.encode("utf-8"))

    if subdirectory is not None:
        # Normalize to either str or bytes using fspath (handles PathLike)
        p = fspath(subdirectory)

        # Try to get a UTF-8 string like Path::to_str()
        if isinstance(p, bytes):
            try:
                p_str = p.decode("utf-8")
            except UnicodeDecodeError:
                p_str = None
        else:
            # Already a str
            p_str = p

        if p_str is not None:
            h.update(b"?subdirectory=")
            h.update(p_str.encode("utf-8"))

    return h.hexdigest()

digest = git_cache_digest(
    repository="https://github.com/agronholm/anyio",
    precise="64b753b19c9a49e3ae395cde457cf82d51f7e999",
    subdirectory=None
)
print(digest)  # lowercase hex, identical to the Rust version
