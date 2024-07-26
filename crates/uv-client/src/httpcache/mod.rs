/*!
A somewhat simplistic implementation of HTTP cache semantics.

This implementation was guided by the following things:

* RFCs 9110 and 9111.
* The `http-cache-semantics` crate. (The implementation here is completely
  different, but the source of `http-cache-semantics` helped guide the
  implementation here and understanding of HTTP caching.)
* A desire for our cache policy to support zero-copy deserialization. That
  is, we want the cached response fast path (where no revalidation request is
  necessary) to avoid any costly deserialization for the cache policy at all.

# Flow

While one has to read the relevant RFCs to get a full understanding of HTTP
caching, doing so is... difficult to say the least. It is at the very least
not quick to do because the semantics are scattered all over the place. But, I
think we can do a quick overview here.

Let's start with the obvious. HTTP caching exists to avoid network requests,
and, if a request is unavoidable, bandwidth. The central actor in HTTP
caching is the `Cache-Control` header, which can exist on *both* requests and
responses. The value of this header is a list of directives that control caching
behavior. They can outright disable it (`no-store`), force cache invalidation
(`no-cache`) or even permit the cache to return responses that are explicitly
stale (`max-stale`).

The main thing that typically drives cache interactions is `max-age`. When set
on a response, this means that the server is willing to let clients hold on to
a response for up to the amount of time in `max-age` before the client must ask
the server for a fresh response. In our case, the main utility of `max-age` is
two fold:

* PyPI sets a `max-age` of 600 seconds (10 minutes) on its responses. As long
  as our cached responses have an age less than this, we can completely avoid
  talking to PyPI at all when we need access to the full set of versions for a
  package.
* Most other assets, like wheels, are forever immutable. They will never
  change. So servers will typically set a very high `max-age`, which means we
  will almost never need to ask the server for permission to reuse our cached
  wheel.

When a cached response exceeds the `max-age` configured on a response, then
we call that response stale. Generally speaking, we won't return responses
from the cache that are known to be stale. (This can be overridden in the
request by adding a `max-stale` cache-control directive, but nothing in uv
does this at time of writing.) When a response is stale, we don't necessarily
need to give up completely. It is at this point that we can send something
called a re-validation request.

A re-validation request includes with it some metadata (usually an "entity tag"
or `etag` for short) that was on the cached response (which is now stale).
When we send this request, the server can compare it with its most up to date
version of the resource. If its entity tag matches the one we gave it (among
other possible criteria), then the server can skip returning the body and
instead just return a small HTTP 304 NOT MODIFIED response. When we get this
type of response, it's the server telling us that our cached response which we
*thought* was stale is no longer stale. It's fresh again and we needn't get a
new copy. We will need to update our stored `CachePolicy` though, since the
HTTP 304 NOT MODIFIED response we got might included updated metadata relevant
to the behavior of caching (like a new `Age` header).

# Scope

In general, the cache semantics implemented below are targeted toward uv's
use case: a private client cache for custom data objects. This constraint
results in a modest simplification in what we need to support. That is, we
don't need to cache the entirety of the request's or response's headers (like
what `http-cache-semantics`) does. Instead, we only need to cache the data
necessary to *make decisions* about HTTP caching.

One example of this is the `Vary` response header. This requires checking the
the headers listed in a cached response have the same value in the original
request and the new request. If the new request has different values for those
headers (as specified in the cached response) than what was in the original
request, then the new request cannot used our cached response. Normally, this
would seemingly require storing all of the original request's headers. But we
only store the headers listed in the response.

Also, since we aren't a proxy, there are a host of proxy-specific rules for
managing headers and data that we needn't care about.

# Zero-copy deserialization

As mentioned above, we would really like our fast path (that is, a cached
response that we deem "fresh" and thus don't need to send a re-validation
request for) to avoid needing to actually deserialize a `CachePolicy`. While a
`CachePolicy` isn't particularly big, it is in our critical path. Yet, we still
need a `CachePolicy` to be able to decide whether a cached response is still
fresh or not. (This decision procedure is non-trivial, so it *probably* doesn't
make too much sense to hack around it with something simpler.)

We attempt to achieve this by implementing the `rkyv` traits for all of our
types. This means that if we read a `Vec<u8>` from a file, then we can very
cheaply turn that into a `rkyvutil::OwnedArchive<CachePolicy>`. Creating that
only requires a quick validation step, but is otherwise free. We can then
use that as-if it were an `Archived<CachePolicy>` (which is an alias for the
`ArchivedCachePolicy` type implicitly introduced by `derive(rkyv::Archive)`).
Crucially, this is why we implement all of our HTTP cache semantics logic on
`ArchivedCachePolicy` and *not* `CachePolicy`. It can be easy to forget this
because `rkyv` does such an amazing job of making its use of archived types
very closely resemble that of the standard types. For example, whenever the
methods below are accessing a field whose type is a `Vec` in the normal type,
what's actually being accessed is a [`rkyv::vec::ArchivedVec`]. Similarly,
for strings, it's [`rkyv::string::ArchivedString`] and not a standard library
`String`. This all works somewhat seamlessly because all of the cache semantics
are generally just read-only operations, but if you stray from the path, you're
likely to get whacked over the head.

One catch here is that we actually want the HTTP cache semantics to be
available on `CachePolicy` too. At least, at time of writing, we do. To
achieve this `CachePolicy::to_archived` is provided, which will serialize the
`CachePolicy` to its archived representation in bytes, and then turn that
into an `OwnedArchive<CachePolicy>` which derefs to `ArchivedCachePolicy`.
This is a little extra cost, but the idea is that a `CachePolicy` (not an
`ArchivedCachePolicy`) should only be used in the slower path (i.e., when you
actually need to make an HTTP request).

[`rkyv::vec::ArchivedVec`]: https://docs.rs/rkyv/0.7.43/rkyv/vec/struct.ArchivedVec.html
[`rkyv::string::ArchivedString`]: https://docs.rs/rkyv/0.7.43/rkyv/string/struct.ArchivedString.html

# Additional reading

* Short introduction to `Cache-Control`: <https://csswizardry.com/2019/03/cache-control-for-civilians/>
* Caching best practcies: <https://jakearchibald.com/2016/caching-best-practices/>
* Overview of HTTP caching: <https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching>
* MDN docs for `Cache-Control`: <https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control>
* The 1997 RFC for HTTP 1.1: <https://www.rfc-editor.org/rfc/rfc2068#section-13>
* The 1999 update to HTTP 1.1: <https://www.rfc-editor.org/rfc/rfc2616.html#section-13>
* The "stale content" cache-control extension: <https://httpwg.org/specs/rfc5861.html>
* HTTP 1.1 caching (superseded by RFC 9111): <https://httpwg.org/specs/rfc7234.html>
* The "immutable" cache-control extension: <https://httpwg.org/specs/rfc8246.html>
* HTTP semantics (If-None-Match, etc.): <https://www.rfc-editor.org/rfc/rfc9110#section-8.8.3>
* HTTP caching (obsoletes RFC 7234): <https://www.rfc-editor.org/rfc/rfc9111.html>
*/

use std::time::{Duration, SystemTime};

use {http::header::HeaderValue, rkyv::bytecheck};

use crate::rkyvutil::OwnedArchive;

use self::control::CacheControl;

mod control;

/// Knobs to configure uv's cache behavior.
///
/// At time of writing, we don't expose any way of modifying these since I
/// suspect we won't ever need to. We split them out into their own type so
/// that they can be shared between `CachePolicyBuilder` and `CachePolicy`.
#[derive(Clone, Debug, rkyv::Archive, rkyv::CheckBytes, rkyv::Deserialize, rkyv::Serialize)]
// Since `CacheConfig` is so simple, we can use itself as the archived type.
// But note that this will fall apart if even something like an Option<u8> is
// added.
#[archive(as = "Self")]
#[repr(C)]
struct CacheConfig {
    shared: bool,
    heuristic_percent: u8,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            // The caching uv does ought to be considered
            // private.
            shared: false,
            // This is only used to heuristically guess at a freshness lifetime
            // when other indicators (such as `max-age` and `Expires` are
            // absent.
            heuristic_percent: 10,
        }
    }
}

/// A builder for constructing a `CachePolicy`.
///
/// A builder can be used directly when spawning fresh HTTP requests
/// without a cached response. A builder is also constructed for you via
/// [`CachePolicy::before_request`] when a cached response exists but is stale.
///
/// The main idea of a builder is that it manages the flow of data needed to
/// construct a `CachePolicy`. That is, you start with an HTTP request, then
/// you get a response and finally a new `CachePolicy`.
#[derive(Debug)]
pub struct CachePolicyBuilder {
    /// The configuration controlling the behavior of the cache.
    config: CacheConfig,
    /// A subset of information from the HTTP request that we will store. This
    /// is needed to make future decisions about cache behavior.
    request: Request,
    /// The full set of request headers. This copy is necessary because the
    /// headers are needed in order to correctly capture the values necessary
    /// to implement the `Vary` check, as per [RFC 9111 S4.1]. The upside is
    /// that this is not actually persisted in a `CachePolicy`. We only need it
    /// until we have the response.
    ///
    /// The precise reason why this copy is intrinsically needed is because
    /// sending a request requires ownership of the request. Yet, we don't know
    /// which header values we need to store in our cache until we get the
    /// response back. Thus, these headers must be persisted until after the
    /// point we've given up ownership of the request.
    ///
    /// [RFC 9111 S4.1]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.1
    request_headers: http::HeaderMap,
}

impl CachePolicyBuilder {
    /// Create a new builder of a cache policy, starting with the request.
    pub fn new(request: &reqwest::Request) -> Self {
        let config = CacheConfig::default();
        let request_headers = request.headers().clone();
        let request = Request::from(request);
        Self {
            config,
            request,
            request_headers,
        }
    }

    /// Return a new policy given the response to the request that this builder
    /// was created with.
    pub fn build(self, response: &reqwest::Response) -> CachePolicy {
        let vary = Vary::from_request_response_headers(&self.request_headers, response.headers());
        CachePolicy {
            config: self.config,
            request: self.request,
            response: Response::from(response),
            vary,
        }
    }
}

/// A value encapsulating the data needed to implement HTTP caching behavior
/// for uv.
///
/// A cache policy is meant to be stored and persisted with the data being
/// cached. It is specifically meant to capture the smallest amount of
/// information needed to determine whether a cached response is stale or not,
/// and the information required to issue a re-validation request.
///
/// This does not provide a complete set of HTTP cache semantics. Notably
/// absent from this (among other things that uv probably doesn't care
/// about it) are proxy cache semantics.
#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct CachePolicy {
    /// The configuration controlling the behavior of the cache.
    config: CacheConfig,
    /// A subset of information from the HTTP request that we will store. This
    /// is needed to make future decisions about cache behavior.
    request: Request,
    /// A subset of information from the HTTP response that we will store. This
    /// is needed to make future decisions about cache behavior.
    response: Response,
    /// This contains the set of vary header names (from the cached response)
    /// and the corresponding values (from the original request) used to verify
    /// whether a new request can utilize a cached response or not. This is
    /// placed outside of `request` and `response` because it contains bits
    /// from both!
    vary: Vary,
}

impl CachePolicy {
    /// Convert this to an owned archive value.
    ///
    /// It's necessary to call this in order to make decisions with this cache
    /// policy. Namely, all of the cache semantics logic is implemented on the
    /// archived types.
    ///
    /// These do incur an extra cost, but this should only be needed when you
    /// don't have an `ArchivedCachePolicy`. And that should only occur when
    /// you're actually performing an HTTP request. In that case, the extra
    /// cost that is done here to convert a `CachePolicy` to its archived form
    /// should be marginal.
    pub fn to_archived(&self) -> OwnedArchive<Self> {
        // There's no way (other than OOM) for serializing this type to fail.
        OwnedArchive::from_unarchived(self).expect("all possible values can be archived")
    }
}

impl ArchivedCachePolicy {
    /// Determines what caching behavior is correct given an existing
    /// `CachePolicy` and a new HTTP request for the resource managed by this
    /// cache policy. This is done as per [RFC 9111 S4].
    ///
    /// Calling this method conceptually corresponds to asking the following
    /// question: "I have a cached response for an incoming HTTP request. May I
    /// return that cached response, or do I need to go back to the progenitor
    /// of that response to determine whether it's still the latest thing?"
    ///
    /// This returns one of three possible behaviors:
    ///
    /// 1. The cached response is still fresh, and the caller may return
    ///    the cached response without issuing an HTTP requests.
    /// 2. The cached response is stale. The caller should send a re-validation
    ///    request and then call `CachePolicy::after_response` to determine whether
    ///    the cached response is actually fresh, or if it's stale and needs to
    ///    be updated.
    /// 3. The given request does not match the cache policy identification.
    ///    Generally speaking, this usually implies a bug with the cache in that
    ///    it loaded a cache policy that does not match the request.
    ///
    /// In the case of (2), the given request is modified in place such that
    /// it is suitable as a revalidation request.
    ///
    /// [RFC 9111 S4]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4
    pub fn before_request(&self, request: &mut reqwest::Request) -> BeforeRequest {
        let now = SystemTime::now();
        // If the response was never storable, then we just bail out
        // completely.
        if !self.is_storable() {
            tracing::trace!(
                "request {} does not match cache request {} because it isn't storable",
                request.url(),
                self.request.uri,
            );
            return BeforeRequest::NoMatch;
        }
        // "When presented with a request, a cache MUST NOT reuse a stored
        // response unless..."
        //
        // "the presented target URI and that of the stored response match,
        // and..."
        if self.request.uri != request.url().as_str() {
            tracing::trace!(
                "request {} does not match cache URL of {}",
                request.url(),
                self.request.uri,
            );
            return BeforeRequest::NoMatch;
        }
        // "the request method associated with the stored response allows it to
        // be used for the presented request, and..."
        if request.method() != http::Method::GET && request.method() != http::Method::HEAD {
            tracing::trace!(
                "method {:?} for request {} is not supported by this cache",
                request.method(),
                request.url(),
            );
            return BeforeRequest::NoMatch;
        }
        // "request header fields nominated by the stored response (if any)
        // match those presented, and..."
        //
        // We don't support the `Vary` header, so if it was set, we
        // conservatively require revalidation.
        if !self.vary.matches(request.headers()) {
            tracing::trace!(
                "request {} does not match cached request because of the 'Vary' header",
                request.url(),
            );
            self.set_revalidation_headers(request);
            return BeforeRequest::Stale(self.new_cache_policy_builder(request));
        }
        // "the stored response does not contain the no-cache directive, unless
        // it is successfully validated, and..."
        if self.response.headers.cc.no_cache {
            self.set_revalidation_headers(request);
            return BeforeRequest::Stale(self.new_cache_policy_builder(request));
        }
        // "the stored response is one of the following: ..."
        //
        // "fresh, or..."
        // "allowed to be served stale, or..."
        if self.is_fresh(now, request) {
            return BeforeRequest::Fresh;
        }
        // "successfully validated."
        //
        // In this case, callers will need to send a revalidation request.
        self.set_revalidation_headers(request);
        BeforeRequest::Stale(self.new_cache_policy_builder(request))
    }

    /// This implements the logic for handling the response to a request that
    /// may be a revalidation request, as per [RFC 9111 S4.3.3] and [RFC 9111
    /// S4.3.4]. That is, the cache policy builder given here should be the one
    /// returned by `CachePolicy::before_request` with the response received
    /// from the origin server for the possibly-revalidating request.
    ///
    /// Even if the request is new (in that there is no response cached
    /// for it), callers may use this routine. But generally speaking,
    /// callers are only supposed to use this routine after getting a
    /// [`BeforeRequest::Stale`].
    ///
    /// The return value indicates whether the cached response is still fresh
    /// (that is, `AfterResponse::NotModified`) or if it has changed (that is,
    /// `AfterResponse::Modified`). In the latter case, the cached response has
    /// been invalidated and the caller should cache the new response. In the
    /// former case, the cached response is still considered fresh.
    ///
    /// In either case, callers should update their cache with the new policy.
    ///
    /// [RFC 9111 S4.3.3]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.3.3
    /// [RFC 9111 S4.3.4]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.3.4
    pub fn after_response(
        &self,
        cache_policy_builder: CachePolicyBuilder,
        response: &reqwest::Response,
    ) -> AfterResponse {
        let mut new_policy = cache_policy_builder.build(response);
        if self.is_modified(&new_policy) {
            AfterResponse::Modified(new_policy)
        } else {
            new_policy.response.status = self.response.status;
            AfterResponse::NotModified(new_policy)
        }
    }

    fn is_modified(&self, new_policy: &CachePolicy) -> bool {
        // From [RFC 9111 S4.3.3],
        //
        // "A 304 (Not Modified) response status code indicates that the stored
        // response can be updated and reused"
        //
        // So if we don't get a 304, then we know our cached response is seen
        // as stale by the origin server.
        //
        // [RFC 9111 S4.3.3]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.3.3
        if new_policy.response.status != 304 {
            tracing::trace!(
                "is modified because status is {:?} and not 304",
                new_policy.response.status
            );
            return true;
        }
        // As per [RFC 9111 S4.3.4], we need to confirm that our validators match. Here,
        // we check `ETag`.
        //
        // [RFC 9111 S4.3.4]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.3.4
        if let Some(old_etag) = self.response.headers.etag.as_ref() {
            if let Some(new_etag) = new_policy.response.headers.etag.as_ref() {
                // We don't support weak validators, so only match if they're
                // both strong.
                if !old_etag.weak && !new_etag.weak && old_etag.value == new_etag.value {
                    tracing::trace!(
                        "not modified because old and new etag values ({:?}) match",
                        new_etag.value,
                    );
                    return false;
                }
            }
        }
        // As per [RFC 9111 S4.3.4], we need to confirm that our validators match. Here,
        // we check `Last-Modified`.
        //
        // [RFC 9111 S4.3.4]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.3.4
        if let Some(old_last_modified) = self.response.headers.last_modified_unix_timestamp.as_ref()
        {
            if let Some(new_last_modified) = new_policy
                .response
                .headers
                .last_modified_unix_timestamp
                .as_ref()
            {
                if old_last_modified == new_last_modified {
                    tracing::trace!(
                        "not modified because modified times ({new_last_modified:?}) match",
                    );
                    return false;
                }
            }
        }
        // As per [RFC 9111 S4.3.4], if we have no validators anywhere, then
        // we can just rely on the HTTP 304 status code and reuse the cached
        // response.
        //
        // [RFC 9111 S4.3.4]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.3.4
        if self.response.headers.etag.is_none()
            && new_policy.response.headers.etag.is_none()
            && self.response.headers.last_modified_unix_timestamp.is_none()
            && new_policy
                .response
                .headers
                .last_modified_unix_timestamp
                .is_none()
        {
            tracing::trace!(
                "not modified because there are no etags or last modified \
                 timestamps, so we assume the 304 status is correct",
            );
            return false;
        }
        true
    }

    /// Sets the relevant headers on the given request so that it can be used
    /// as a revalidation request. As per [RFC 9111 S4.3.1], this permits the
    /// origin server to check if the content is different from our cached
    /// response. If it isn't, then the origin server can return an HTTP 304
    /// NOT MODIFIED status, which avoids the need to re-transmit the response
    /// body. That is, it indicates that our cached response is still fresh.
    ///
    /// This will always use a strong etag validator if it's present on the
    /// cached response. If the given request already has an etag validator
    /// on it, this routine will add to it and not replace it.
    ///
    /// In contrast, if the request already has the `If-Modified-Since` header
    /// set, then this will not change or replace it. If it's not set, then one
    /// is added if the cached response had a valid `Last-Modified` header.
    ///
    /// [RFC 9111 S4.3.1]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.3.1
    fn set_revalidation_headers(&self, request: &mut reqwest::Request) {
        // As per [RFC 9110 13.1.2] and [RFC 9111 S4.3.1], if our stored
        // response has an etag, we should send it back via the `If-None-Match`
        // header. The idea is that the server should only "do" the request if
        // none of the tags match. If there is a match, then the server can
        // return HTTP 304 indicating that our stored response is still fresh.
        //
        // [RFC 9110 S13.1.2]: https://www.rfc-editor.org/rfc/rfc9110#section-13.1.2
        // [RFC 9111 S4.3.1]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.3.1
        if let Some(etag) = self.response.headers.etag.as_ref() {
            // We don't support weak validation principally because we want to
            // be notified if there was a change in the content. Namely, from
            // RFC 9110 S13.1.2: "... weak entity tags can be used for cache
            // validation even if there have been changes to the representation
            // data."
            if !etag.weak {
                if let Ok(header) = HeaderValue::from_bytes(&etag.value) {
                    request.headers_mut().append("if-none-match", header);
                }
            }
        }
        // We also set `If-Modified-Since` as per [RFC 9110 S13.1.3] and [RFC
        // 9111 S4.3.1]. Generally, `If-None-Match` will override this, but we
        // set it in case `If-None-Match` is not supported.
        //
        // [RFC 9110 S13.1.3]: https://www.rfc-editor.org/rfc/rfc9110#section-13.1.3
        // [RFC 9111 S4.3.1]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.3.1
        if !request.headers().contains_key("if-modified-since") {
            if let Some(&last_modified_unix_timestamp) =
                self.response.headers.last_modified_unix_timestamp.as_ref()
            {
                if let Some(last_modified) = unix_timestamp_to_header(last_modified_unix_timestamp)
                {
                    request
                        .headers_mut()
                        .insert("if-modified-since", last_modified);
                }
            }
        }
    }

    /// Returns true if and only if the response is storable as per
    /// [RFC 9111 S3].
    ///
    /// [RFC 9111 S3]: https://www.rfc-editor.org/rfc/rfc9111.html#section-3
    pub fn is_storable(&self) -> bool {
        // In the absence of other signals, we are limited to caching responses
        // with a code that is heuristically cacheable as per [RFC 9110 S15.1].
        //
        // [RFC 9110 S15.1]: https://www.rfc-editor.org/rfc/rfc9110#section-15.1
        const HEURISTICALLY_CACHEABLE_STATUS_CODES: &[u16] =
            &[200, 203, 204, 206, 300, 301, 308, 404, 405, 410, 414, 501];

        // N.B. This routine could be "simpler", but we bias toward
        // following the flow of logic as closely as possible as written
        // in RFC 9111 S3.

        // "the request method is understood by the cache"
        //
        // We just don't bother with anything that isn't GET.
        if !matches!(
            self.request.method,
            ArchivedMethod::Get | ArchivedMethod::Head
        ) {
            tracing::trace!(
                "cached request {} is not storable because of its method {:?}",
                self.request.uri,
                self.request.method
            );
            return false;
        }
        // "the response status code is final"
        //
        // ... and we'll put more restrictions on status code
        // below, but we can bail out early here.
        if !self.response.has_final_status() {
            tracing::trace!(
                "cached request {} is not storable because its response has \
                 non-final status code {:?}",
                self.request.uri,
                self.response.status,
            );
            return false;
        }
        // "if the response status code is 206 or 304, or the must-understand
        // cache directive (see Section 5.2.2.3) is present: the cache
        // understands the response status code"
        //
        // We don't currently support `must-understand`. We also don't support
        // partial content (206). And 304 not modified shouldn't be cached
        // itself.
        if self.response.status == 206 || self.response.status == 304 {
            tracing::trace!(
                "cached request {} is not storable because its response has \
                 unsupported status code {:?}",
                self.request.uri,
                self.response.status,
            );
            return false;
        }
        // "The no-store request directive indicates that a cache MUST NOT
        // store any part of either this request or any response to it."
        //
        // (This is from RFC 9111 S5.2.1.5, and doesn't seem to be mentioned in
        // S3.)
        if self.request.headers.cc.no_store {
            tracing::trace!(
                "cached request {} is not storable because its request has \
                 a 'no-store' cache-control directive",
                self.request.uri,
            );
            return false;
        }
        // "the no-store cache directive is not present in the response"
        if self.response.headers.cc.no_store {
            tracing::trace!(
                "cached request {} is not storable because its response has \
                 a 'no-store' cache-control directive",
                self.request.uri,
            );
            return false;
        }
        // "if the cache is shared ..."
        if self.config.shared {
            // "if the cache is shared: the private response directive is either
            // not present or allows a shared cache to store a modified response"
            //
            // We don't support more granular "private" directives (which allow
            // caching all of a private HTTP response in a shared cache only after
            // removing some subset of the response's headers that are deemed
            // private).
            if self.response.headers.cc.private {
                tracing::trace!(
                    "cached request {} is not storable because this is a shared \
                     cache and its response has a 'private' cache-control directive",
                    self.request.uri,
                );
                return false;
            }
            // "if the cache is shared: the Authorization header field is not
            // present in the request or a response directive is present that
            // explicitly allows shared caching"
            if self.request.headers.authorization && !self.allows_authorization_storage() {
                tracing::trace!(
                    "cached request {} is not storable because this is a shared \
                     cache and the request has an 'Authorization' header set and \
                     the response has indicated that caching requests with an \
                     'Authorization' header is allowed",
                    self.request.uri,
                );
                return false;
            }
        }

        // "the response contains at least one of the following ..."
        //
        // "a public response directive"
        if self.response.headers.cc.public {
            tracing::trace!(
                "cached request {} is storable because its response has \
                 a 'public' cache-control directive",
                self.request.uri,
            );
            return true;
        }
        // "a private response directive, if the cache is not shared"
        if !self.config.shared && self.response.headers.cc.private {
            tracing::trace!(
                "cached request {} is storable because this is a shared cache \
                 and its response has a 'private' cache-control directive",
                self.request.uri,
            );
            return true;
        }
        // "an Expires header field"
        if self.response.headers.expires_unix_timestamp.is_some() {
            tracing::trace!(
                "cached request {} is storable because its response has an \
                 'Expires' header set",
                self.request.uri,
            );
            return true;
        }
        // "a max-age response directive"
        if self.response.headers.cc.max_age_seconds.is_some() {
            tracing::trace!(
                "cached request {} is storable because its response has an \
                 'max-age' cache-control directive",
                self.request.uri,
            );
            return true;
        }
        // "if the cache is shared: an s-maxage response directive"
        if self.config.shared && self.response.headers.cc.s_maxage_seconds.is_some() {
            tracing::trace!(
                "cached request {} is storable because this is a shared cache \
                 and its response has a 's-maxage' cache-control directive",
                self.request.uri,
            );
            return true;
        }
        // "a cache extension that allows it to be cached"
        // ... we don't support any extensions.
        //
        // "a status code that is defined as heuristically cacheable"
        if HEURISTICALLY_CACHEABLE_STATUS_CODES.contains(&self.response.status) {
            tracing::trace!(
                "cached request {} is storable because its response has a \
                 heuristically cacheable status code {:?}",
                self.request.uri,
                self.response.status,
            );
            return true;
        }
        tracing::trace!(
            "cached response {} is not storable because it does not meet any \
             of the necessary criteria (e.g., it doesn't have an 'Expires' \
             header set or a 'max-age' cache-control directive)",
            self.request.uri,
        );
        false
    }

    /// Returns true when a response is storable even if it has an
    /// `Authorization` header, as per [RFC 9111 S3.5].
    ///
    /// [RFC 9111 S3.5]: https://www.rfc-editor.org/rfc/rfc9111.html#section-3.5
    fn allows_authorization_storage(&self) -> bool {
        self.response.headers.cc.must_revalidate
            || self.response.headers.cc.public
            || self.response.headers.cc.s_maxage_seconds.is_some()
    }

    /// Returns true if the response is considered fresh as per [RFC 9111
    /// S4.2]. If the response is not fresh, then it considered stale and ought
    /// to be revalidated with the origin server.
    ///
    /// [RFC 9111 S4.2]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.2
    fn is_fresh(&self, now: SystemTime, request: &reqwest::Request) -> bool {
        let freshness_lifetime = self.freshness_lifetime().as_secs();
        let age = self.age(now).as_secs();

        // Per RFC 8246, the `immutable` directive means that a reload from an
        // end user should not result in a revlalidation request. Indeed, the
        // `immutable` directive seems to imply that clients should never talk
        // to the origin server until the cached response is stale with respect
        // to its freshness lifetime (as set by the server).
        //
        // A *force* reload from an end user should override this, but we
        // currently have no path for that in this implementation. Instead, we
        // just interpret `immutable` as meaning that any directives on the
        // new request that would otherwise result in sending a revalidation
        // request are ignored.
        //
        // [RFC 8246]: https://httpwg.org/specs/rfc8246.html
        if !self.response.headers.cc.immutable {
            let reqcc = request
                .headers()
                .get_all("cache-control")
                .iter()
                .collect::<CacheControl>();

            // As per [RFC 9111 S5.2.1.4], if the request has `no-cache`, then we should
            // respect that.
            //
            // [RFC 9111 S5.2.1.4]: https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.1.4
            if reqcc.no_cache {
                tracing::trace!(
                    "request {} does not have a fresh cache because \
                 it has a 'no-cache' cache-control directive",
                    request.url(),
                );
                return false;
            }

            // If the request has a max-age directive, then we should respect that
            // as per [RFC 9111 S5.2.1.1].
            //
            // [RFC 9111 S5.2.1.1]: https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.1.1
            if let Some(&max_age) = reqcc.max_age_seconds.as_ref() {
                if age > max_age {
                    tracing::trace!(
                        "request {} does not have a fresh cache because \
                     the cached response's age is {} seconds and the max age \
                     allowed by the request is {} seconds",
                        request.url(),
                        age,
                        max_age,
                    );
                    return false;
                }
            }

            // If the request has a min-fresh directive, then we only consider a
            // cached response fresh if the remaining time it has to live exceeds
            // the threshold provided, as per [RFC 9111 S5.2.1.3].
            //
            // [RFC 9111 S5.2.1.3]: https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.1.3
            if let Some(&min_fresh) = reqcc.min_fresh_seconds.as_ref() {
                let time_to_live = freshness_lifetime.saturating_sub(unix_timestamp(now));
                if time_to_live < min_fresh {
                    tracing::trace!(
                        "request {} does not have a fresh cache because \
                     the request set a 'min-fresh' cache-control directive, \
                     and its time-to-live is {} seconds but it needs to be \
                     at least {} seconds",
                        request.url(),
                        time_to_live,
                        min_fresh,
                    );
                    // Note that S5.2.1.3 does not say that max-stale overrides
                    // this, so we ignore it here.
                    return false;
                }
            }
        }
        if age > freshness_lifetime {
            let allows_stale = self.allows_stale(now);
            if !allows_stale {
                tracing::trace!(
                    "request {} does not have a fresh cache because \
                     its age is {} seconds, it is greater than the freshness \
                     lifetime of {} seconds and stale cached responses are not \
                     allowed",
                    request.url(),
                    age,
                    freshness_lifetime,
                );
                return false;
            }
        }
        true
    }

    /// Returns true if we're allowed to serve a stale response, as per [RFC
    /// 9111 S4.2.4].
    ///
    /// [RFC 9111 S4.2.4]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.2.4
    fn allows_stale(&self, now: SystemTime) -> bool {
        // As per [RFC 9111 S5.2.2.2], if `must-revalidate` is present, then
        // caches cannot reuse a stale response without talking to the server
        // first. Note that RFC 9111 doesn't seem to say anything about the
        // interaction between must-revalidate and max-stale, so we assume that
        // must-revalidate takes precedent.
        //
        // [RFC 9111 S5.2.2.2]: https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.2.2
        if self.response.headers.cc.must_revalidate {
            tracing::trace!(
                "cached request {} has a cached response that does not \
                 permit staleness because the response has a 'must-revalidate' \
                 cache-control directive set",
                self.request.uri,
            );
            return false;
        }
        if let Some(&max_stale) = self.request.headers.cc.max_stale_seconds.as_ref() {
            // As per [RFC 9111 S5.2.1.2], if the client has max-stale set,
            // then stale responses are allowed, but only if they are stale
            // within a given threshold.
            //
            // [RFC 9111 S5.2.1.2]: https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.1.2
            let stale_amount = self
                .age(now)
                .as_secs()
                .saturating_sub(self.freshness_lifetime().as_secs());
            if stale_amount <= max_stale {
                tracing::trace!(
                    "cached request {} has a cached response that allows staleness \
                     in this case because the stale amount is {} seconds and the \
                     'max-stale' cache-control directive set by the cached request \
                     is {} seconds",
                    self.request.uri,
                    stale_amount,
                    max_stale,
                );
                return true;
            }
        }
        // As per [RFC 9111 S4.2.4], we shouldn't use stale responses unless
        // we're explicitly allowed to (e.g., via `max-stale` above):
        //
        // "A cache MUST NOT generate a stale response unless it is
        // disconnected or doing so is explicitly permitted by the client or
        // origin server..."
        //
        // [RFC 9111 S4.2.4]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.2.4
        tracing::trace!(
            "cached request {} has a cached response that does not allow staleness",
            self.request.uri,
        );
        false
    }

    /// Returns the age of the HTTP response as per [RFC 9111 S4.2.3].
    ///
    /// The age of a response, essentially, refers to how long it has been
    /// since the response was created by the origin server. The age is used
    /// to compare with the freshness lifetime of the response to determine
    /// whether the response is fresh or stale.
    ///
    /// [RFC 9111 S4.2.3]: https://www.rfc-editor.org/rfc/rfc9111.html#name-calculating-age
    fn age(&self, now: SystemTime) -> Duration {
        // RFC 9111 S4.2.3
        let apparent_age = self
            .response
            .unix_timestamp
            .saturating_sub(self.response.header_date());
        let response_delay = self
            .response
            .unix_timestamp
            .saturating_sub(self.request.unix_timestamp);
        let corrected_age_value = self.response.header_age().saturating_add(response_delay);
        let corrected_initial_age = apparent_age.max(corrected_age_value);
        let resident_age = unix_timestamp(now).saturating_sub(self.response.unix_timestamp);
        let current_age = corrected_initial_age + resident_age;
        Duration::from_secs(current_age)
    }

    /// Returns how long a response should be considered "fresh" as per
    /// [RFC 9111 S4.2.1]. When this returns false, the response should be
    /// considered stale and the client should revalidate with the server.
    ///
    /// If there are no indicators of a response's freshness lifetime, then
    /// this returns `0`. That is, the response will be considered stale in all
    /// cases.
    ///
    /// [RFC 9111 S4.2.1]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.2.1
    fn freshness_lifetime(&self) -> Duration {
        if self.config.shared {
            if let Some(&s_maxage) = self.response.headers.cc.s_maxage_seconds.as_ref() {
                return Duration::from_secs(s_maxage);
            }
        }
        if let Some(&max_age) = self.response.headers.cc.max_age_seconds.as_ref() {
            return Duration::from_secs(max_age);
        }
        if let Some(&expires) = self.response.headers.expires_unix_timestamp.as_ref() {
            return Duration::from_secs(expires.saturating_sub(self.response.header_date()));
        }
        if let Some(&last_modified) = self.response.headers.last_modified_unix_timestamp.as_ref() {
            let interval = self.response.header_date().saturating_sub(last_modified);
            let percent = u64::from(self.config.heuristic_percent);
            return Duration::from_secs(interval.saturating_mul(percent).saturating_div(100));
        }
        // Without any indicators as to the freshness lifetime, we act
        // conservatively and use a value that will always result in a response
        // being treated as stale.
        Duration::ZERO
    }

    fn new_cache_policy_builder(&self, request: &reqwest::Request) -> CachePolicyBuilder {
        let request_headers = request.headers().clone();
        CachePolicyBuilder {
            config: self.config.clone(),
            request: Request::from(request),
            request_headers,
        }
    }
}

/// The result of calling [`CachePolicy::before_request`].
///
/// This dictates what the caller should do next by indicating whether the
/// cached response is stale or not.
#[derive(Debug)]
pub enum BeforeRequest {
    /// The cached response is still fresh, and the caller may return the
    /// cached response without issuing an HTTP requests.
    Fresh,
    /// The cached response is stale. The caller should send a re-validation
    /// request and then call `CachePolicy::after_response` to determine
    /// whether the cached response is actually fresh, or if it's stale and
    /// needs to be updated.
    Stale(CachePolicyBuilder),
    /// The given request does not match the cache policy identification.
    /// Generally speaking, this is usually implies a bug with the cache in
    /// that it loaded a cache policy that does not match the request.
    NoMatch,
}

/// The result of called [`CachePolicy::after_response`].
///
/// This is meant to report whether a revalidation request was successful or
/// not. If it was, then a `AfterResponse::NotModified` is returned. Otherwise,
/// the server determined the cached response was truly stale and in need of
/// updated.
#[derive(Debug)]
pub enum AfterResponse {
    /// The cached response is still fresh.
    NotModified(CachePolicy),
    /// The cached response has been invalidated and needs to be updated with
    /// the new data in the response to the revalidation request.
    Modified(CachePolicy),
}

#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
struct Request {
    uri: String,
    method: Method,
    headers: RequestHeaders,
    unix_timestamp: u64,
}

impl<'a> From<&'a reqwest::Request> for Request {
    fn from(from: &'a reqwest::Request) -> Self {
        Self {
            uri: from.url().to_string(),
            method: Method::from(from.method()),
            headers: RequestHeaders::from(from.headers()),
            unix_timestamp: unix_timestamp(SystemTime::now()),
        }
    }
}

#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
struct RequestHeaders {
    /// The cache control directives from the `Cache-Control` header.
    cc: CacheControl,
    /// This is set to `true` only when an `Authorization` header is present.
    /// We don't need to record the value.
    authorization: bool,
}

impl<'a> From<&'a http::HeaderMap> for RequestHeaders {
    fn from(from: &'a http::HeaderMap) -> Self {
        Self {
            cc: from.get_all("cache-control").iter().collect(),
            authorization: from.contains_key("authorization"),
        }
    }
}

/// The HTTP method used on a request.
///
/// We don't both representing methods of requests whose responses we won't
/// cache. Instead, we treat them as "unrecognized" and consider the responses
/// not-storable.
#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
#[repr(u8)]
enum Method {
    Get,
    Head,
    Unrecognized,
}

impl<'a> From<&'a http::Method> for Method {
    fn from(from: &'a http::Method) -> Self {
        if from == http::Method::GET {
            Self::Get
        } else if from == http::Method::HEAD {
            Self::Head
        } else {
            Self::Unrecognized
        }
    }
}

#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
struct Response {
    status: u16,
    headers: ResponseHeaders,
    unix_timestamp: u64,
}

impl ArchivedResponse {
    /// Returns the "age" header value on this response, with a fallback of `0`
    /// if the header doesn't exist or is invalid, as per [RFC 9111 S4.2.3].
    ///
    /// Note that this does not reflect the true "age" of a response. That
    /// is computed via `ArchivedCachePolicy::age` as it may need additional
    /// information (such as the request time).
    ///
    /// [RFC 9111 S4.2.3]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.2.3
    fn header_age(&self) -> u64 {
        self.headers.age_seconds.unwrap_or(0)
    }

    /// Returns the "date" header value on this response, with a fallback to
    /// the time the response was received as per [RFC 9110 S6.6.1].
    ///
    /// [RFC 9110 S6.6.1]: https://www.rfc-editor.org/rfc/rfc9110#section-6.6.1
    fn header_date(&self) -> u64 {
        self.headers
            .date_unix_timestamp
            .unwrap_or(self.unix_timestamp)
    }

    /// Returns true when this response has a status code that is considered
    /// "final" as per [RFC 9110 S15].
    ///
    /// [RFC 9110 S15]: https://www.rfc-editor.org/rfc/rfc9110#section-15
    fn has_final_status(&self) -> bool {
        self.status >= 200
    }
}

impl<'a> From<&'a reqwest::Response> for Response {
    fn from(from: &'a reqwest::Response) -> Self {
        Self {
            status: from.status().as_u16(),
            headers: ResponseHeaders::from(from.headers()),
            unix_timestamp: unix_timestamp(SystemTime::now()),
        }
    }
}

#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
struct ResponseHeaders {
    /// The directives from the `Cache-Control` header.
    cc: CacheControl,
    /// The value of the `Age` header corresponding to `age_value` as defined
    /// in [RFC 9111 S4.2.3]. If the `Age` header is not present, it should be
    /// interpreted at `0`.
    ///
    /// [RFC 9111 S4.2.3]: https://www.rfc-editor.org/rfc/rfc9111.html#name-calculating-age
    age_seconds: Option<u64>,
    /// This is `date_value` from [RFC 9111 S4.2.3], which says it corresponds
    /// to the `Date` header on a response as defined in [RFC 7231 S7.1.1.2].
    /// In RFC 7231, if the `Date` header is not present, then the recipient
    /// should treat its value as equivalent to the time the response was
    /// received. In this case, that would be `Response::unix_timestamp`.
    ///
    /// [RFC 9111 S4.2.3]: https://www.rfc-editor.org/rfc/rfc9111.html#name-calculating-age
    /// [RFC 7231 S7.1.1.2]: https://httpwg.org/specs/rfc7231.html#header.date
    date_unix_timestamp: Option<u64>,
    /// This is from the `Expires` header as per [RFC 9111 S5.3]. Note that this
    /// is overridden by the presence of either the `max-age` or `s-maxage` cache
    /// control directives.
    ///
    /// If an `Expires` header was present but did not contain a valid RFC 2822
    /// datetime, then this is set to `Some(0)`. (That is, some time in the
    /// past, which implies the response has already expired.)
    ///
    /// [RFC 9111 S5.3]: https://www.rfc-editor.org/rfc/rfc9111.html#section-5.3
    expires_unix_timestamp: Option<u64>,
    /// The date from the `Last-Modified` header as specified in [RFC 9110 S8.8.2]
    /// in RFC 2822 format. It's used to compute a heuristic freshness lifetime for
    /// the response when other indicators are missing as per [RFC 9111 S4.2.2].
    ///
    /// [RFC 9110 S8.8.2]: https://www.rfc-editor.org/rfc/rfc9110#section-8.8.2
    /// [RFC 9111 S4.2.2]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.2.2
    last_modified_unix_timestamp: Option<u64>,
    /// The "entity tag" from the response as per [RFC 9110 S8.8.3], which is
    /// used in revalidation requests.
    ///
    /// [RFC 9110 S8.8.3]: https://www.rfc-editor.org/rfc/rfc9110#section-8.8.3
    etag: Option<ETag>,
}

impl<'a> From<&'a http::HeaderMap> for ResponseHeaders {
    fn from(from: &'a http::HeaderMap) -> Self {
        Self {
            cc: from.get_all("cache-control").iter().collect(),
            age_seconds: from
                .get("age")
                .and_then(|header| parse_seconds(header.as_bytes())),
            date_unix_timestamp: from
                .get("date")
                .and_then(|header| header.to_str().ok())
                .and_then(rfc2822_to_unix_timestamp),
            expires_unix_timestamp: from
                .get("expires")
                .and_then(|header| header.to_str().ok())
                .and_then(rfc2822_to_unix_timestamp),
            last_modified_unix_timestamp: from
                .get("last-modified")
                .and_then(|header| header.to_str().ok())
                .and_then(rfc2822_to_unix_timestamp),
            etag: from
                .get("etag")
                .map(|header| ETag::parse(header.as_bytes())),
        }
    }
}

#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
struct ETag {
    /// The actual `ETag` validator value.
    ///
    /// This is received in the response, recorded as part of the cache policy
    /// and then sent back in a re-validation request. This is the "best"
    /// way for an HTTP server to return an HTTP 304 NOT MODIFIED status,
    /// indicating that our cached response is still fresh.
    value: Vec<u8>,
    /// When `weak` is true, this etag is considered a "weak" validator. In
    /// effect, it provides weaker semantics than a "strong" validator. As per
    /// [RFC 9110 S8.8.1]:
    ///
    /// "In contrast, a "weak validator" is representation metadata that might
    /// not change for every change to the representation data. This weakness
    /// might be due to limitations in how the value is calculated (e.g.,
    /// clock resolution), an inability to ensure uniqueness for all possible
    /// representations of the resource, or a desire of the resource owner to
    /// group representations by some self-determined set of equivalency rather
    /// than unique sequences of data."
    ///
    /// We don't currently support weak validation.
    ///
    /// [RFC 9110 S8.8.1]: https://www.rfc-editor.org/rfc/rfc9110#section-8.8.1-6
    weak: bool,
}

impl ETag {
    /// Parses an `ETag` from a header value.
    ///
    /// We are a little permissive here and allow arbitrary bytes,
    /// where as [RFC 9110 S8.8.3] is a bit more restrictive.
    ///
    /// [RFC 9110 S8.8.3]: https://www.rfc-editor.org/rfc/rfc9110#section-8.8.3
    fn parse(header_value: &[u8]) -> Self {
        let (value, weak) = if header_value.starts_with(b"W/") {
            (&header_value[2..], true)
        } else {
            (header_value, false)
        };
        Self {
            value: value.to_vec(),
            weak,
        }
    }
}

/// Represents the `Vary` header on a cached response, as per [RFC 9110
/// S12.5.5] and [RFC 9111 S4.1].
///
/// This permits responses from the server to express things like, "only used
/// an existing cached response if the request from the client has the same
/// header values for the headers listed in `Vary` as in the original request."
///
/// [RFC 9110 S12.5.5]: https://www.rfc-editor.org/rfc/rfc9110#section-12.5.5
/// [RFC 9111 S4.1]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.1
#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
struct Vary {
    fields: Vec<VaryField>,
}

impl Vary {
    /// Returns a `Vary` header value that will never match any request.
    fn always_fails_to_match() -> Self {
        Self {
            fields: vec![VaryField {
                name: "*".to_string(),
                value: vec![],
            }],
        }
    }

    fn from_request_response_headers(
        request: &http::HeaderMap,
        response: &http::HeaderMap,
    ) -> Self {
        // Parses the `Vary` header as per [RFC 9110 S12.5.5].
        //
        // [RFC 9110 S12.5.5]: https://www.rfc-editor.org/rfc/rfc9110#section-12.5.5
        let mut fields = vec![];
        for header in response.get_all("vary") {
            let Ok(csv) = header.to_str() else { continue };
            for header_name in csv.split(',') {
                let header_name = header_name.trim().to_ascii_lowercase();
                // When we see a `*`, that means a failed match is an
                // inevitability, regardless of anything else. So just give up
                // and return a `Vary` that will never match.
                if header_name == "*" {
                    return Self::always_fails_to_match();
                }
                let value = request
                    .get(&header_name)
                    .map(|header| header.as_bytes().to_vec())
                    .unwrap_or_default();
                fields.push(VaryField {
                    name: header_name,
                    value,
                });
            }
        }
        Self { fields }
    }
}

impl ArchivedVary {
    /// Returns true only when the `Vary` header on a cached response satisfies
    /// the request header values given, as per [RFC 9111 S4.1].
    ///
    /// [RFC 9111 S4.1]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.1
    fn matches(&self, request_headers: &http::HeaderMap) -> bool {
        for field in self.fields.iter() {
            // A `*` anywhere means the match always fails.
            if field.name == "*" {
                return false;
            }
            let request_header_value = request_headers
                .get(field.name.as_str())
                .map_or(&b""[..], |header| header.as_bytes());
            if field.value.as_slice() != request_header_value {
                return false;
            }
        }
        true
    }
}

/// A single field and value in a `Vary` header set by the response,
/// as per [RFC 9111 S4.1].
///
/// The `name` of the field comes from the `Vary` header in the response,
/// while the value of the field comes from the value of the header with the
/// same `name` in the original request. These field and value pairs are then
/// compared with new incoming requests. If there is a mismatch, then the
/// cached response cannot be used.
///
/// [RFC 9111 S4.1]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.1
#[derive(Debug, rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
struct VaryField {
    name: String,
    value: Vec<u8>,
}

fn unix_timestamp(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .expect("UNIX_EPOCH is as early as it gets")
        .as_secs()
}

fn rfc2822_to_unix_timestamp(s: &str) -> Option<u64> {
    rfc2822_to_datetime(s).and_then(|dt| u64::try_from(dt.timestamp()).ok())
}

fn rfc2822_to_datetime(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc2822(s)
        .ok()
        .map(|dt| dt.to_utc())
}

fn unix_timestamp_to_header(seconds: u64) -> Option<HeaderValue> {
    unix_timestamp_to_rfc2822(seconds).and_then(|string| HeaderValue::from_str(&string).ok())
}

fn unix_timestamp_to_rfc2822(seconds: u64) -> Option<String> {
    unix_timestamp_to_datetime(seconds).map(|dt| dt.to_rfc2822())
}

fn unix_timestamp_to_datetime(seconds: u64) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::from_timestamp(i64::try_from(seconds).ok()?, 0)
}

fn parse_seconds(value: &[u8]) -> Option<u64> {
    if !value.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(value).ok()?.parse().ok()
}
