#!/usr/bin/env python3

"""Provider adapter interface for ship art generation.

Defines the abstract provider interface and a fake provider used by all
automated tests.  The first real provider adapter uses the Gemini
reference-image flow, but provider endpoint and model are configuration,
not permanent schema.

API keys are read only from the environment and never written to logs or
provenance.

Phase 3 of ``docs/SHIP-ART-IMPLEMENTATION-PLAN.md``.
"""

from __future__ import annotations

import base64
import io
import json
import os
import time
import urllib.request
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any


# ---------------------------------------------------------------------------
# Provider request/result
# ---------------------------------------------------------------------------


@dataclass
class ProviderRequest:
    """A single image generation request to the provider."""

    prompt: str
    reference_image_b64: str | None = None
    model: str = "gemini-2.5-flash-image"


@dataclass
class ProviderResult:
    """Result of a provider image generation call."""

    success: bool
    image_data: bytes | None = None
    error: str = ""
    attempts: int = 0
    timed_out: bool = False


# ---------------------------------------------------------------------------
# Provider interface
# ---------------------------------------------------------------------------


class ProviderAdapter(ABC):
    """Abstract interface for image generation providers."""

    @property
    @abstractmethod
    def name(self) -> str:
        """Human-readable provider name."""

    @abstractmethod
    def generate(self, request: ProviderRequest, retries: int = 3) -> ProviderResult:
        """Generate an image from a prompt, optionally with a reference image.

        Args:
            request: The generation request.
            retries: Maximum number of retry attempts on transient failure.

        Returns:
            ProviderResult with image_data on success or error details.
        """


# ---------------------------------------------------------------------------
# Fake provider (for testing)
# ---------------------------------------------------------------------------


@dataclass
class FakeProviderConfig:
    """Configuration for the fake provider's behavior."""

    # If set, return this image data on success.
    image_data: bytes | None = None
    # If set, fail with this error message on every call.
    fail_always: str = ""
    # If set, fail on the first N attempts, then succeed.
    fail_first_n: int = 0
    # If set, simulate a timeout.
    timeout: bool = False
    # If set, return malformed JSON.
    malformed_json: bool = False
    # If set, return a response with no image payload.
    missing_image: bool = False
    # If set, return a response with no candidates.
    missing_candidates: bool = False
    # Sleep this many seconds per attempt (to simulate latency).
    delay: float = 0.0


class FakeProvider(ProviderAdapter):
    """Fake provider for automated testing.

    Configurable to simulate success, timeout, malformed JSON, missing image
    payload, bounded retry, and validation retry scenarios.
    """

    def __init__(self, config: FakeProviderConfig | None = None):
        self.config = config or FakeProviderConfig()
        self.call_count = 0
        self.requests: list[ProviderRequest] = []

    @property
    def name(self) -> str:
        return "fake"

    def generate(self, request: ProviderRequest, retries: int = 3) -> ProviderResult:
        self.requests.append(request)
        self.call_count += 1

        if self.config.delay > 0:
            time.sleep(self.config.delay)

        # Always fail.
        if self.config.fail_always:
            return ProviderResult(
                success=False,
                error=self.config.fail_always,
                attempts=1,
            )

        # Timeout.
        if self.config.timeout:
            return ProviderResult(
                success=False,
                error="timeout",
                attempts=1,
                timed_out=True,
            )

        # Fail first N attempts.
        if self.config.fail_first_n > 0 and self.call_count <= self.config.fail_first_n:
            return ProviderResult(
                success=False,
                error=f"simulated failure (attempt {self.call_count})",
                attempts=self.call_count,
            )

        # Missing candidates (malformed response).
        if self.config.missing_candidates:
            return ProviderResult(
                success=False,
                error="no candidates in response",
                attempts=1,
            )

        # Missing image payload.
        if self.config.missing_image:
            return ProviderResult(
                success=False,
                error="no image in response",
                attempts=1,
            )

        # Malformed JSON.
        if self.config.malformed_json:
            return ProviderResult(
                success=False,
                error="malformed JSON response",
                attempts=1,
            )

        # Success.
        image_data = self.config.image_data or b""
        return ProviderResult(
            success=True,
            image_data=image_data,
            attempts=1,
        )


# ---------------------------------------------------------------------------
# Gemini provider (real, requires API key from env)
# ---------------------------------------------------------------------------


class GeminiProvider(ProviderAdapter):
    """Gemini reference-image provider adapter.

    API key is read only from the GEMINI_API_KEY environment variable.
    The key is never written to logs or provenance.
    """

    DEFAULT_MODEL = "gemini-2.5-flash-image"
    BASE_URL = "https://generativelanguage.googleapis.com/v1beta/models"
    REQUEST_TIMEOUT = 120

    def __init__(self, model: str | None = None, api_key: str | None = None):
        self._model = model or self.DEFAULT_MODEL
        self._api_key = api_key  # May be None; resolved at call time.

    @property
    def name(self) -> str:
        return "gemini"

    @property
    def model(self) -> str:
        return self._model

    def _get_api_key(self) -> str:
        """Resolve the API key from the environment."""
        key = self._api_key or os.environ.get("GEMINI_API_KEY", "")
        if not key:
            raise RuntimeError("GEMINI_API_KEY not set")
        return key

    def generate(self, request: ProviderRequest, retries: int = 3) -> ProviderResult:
        """Generate an image via the Gemini API."""
        try:
            api_key = self._get_api_key()
        except RuntimeError as e:
            return ProviderResult(success=False, error=str(e), attempts=0)

        url = f"{self.BASE_URL}/{self._model}:generateContent"

        parts: list[dict[str, Any]] = []
        if request.reference_image_b64:
            parts.append({
                "inlineData": {
                    "mimeType": "image/png",
                    "data": request.reference_image_b64,
                }
            })
        parts.append({"text": request.prompt})

        body = json.dumps({
            "contents": [{"parts": parts}],
            "generationConfig": {"responseModalities": ["IMAGE", "TEXT"]},
        }).encode()

        for attempt in range(retries):
            if attempt > 0:
                wait = 30 * attempt
                time.sleep(wait)

            try:
                req = urllib.request.Request(
                    url,
                    data=body,
                    headers={
                        "Content-Type": "application/json",
                        "x-goog-api-key": api_key,  # Key in header only, never logged.
                    },
                )
                with urllib.request.urlopen(req, timeout=self.REQUEST_TIMEOUT) as resp:
                    data = json.loads(resp.read())
            except Exception as e:
                continue

            candidates = data.get("candidates", [])
            if not candidates:
                continue
            parts_resp = candidates[0].get("content", {}).get("parts", [])
            for p in parts_resp:
                if "inlineData" in p:
                    img_data = base64.b64decode(p["inlineData"]["data"])
                    return ProviderResult(
                        success=True,
                        image_data=img_data,
                        attempts=attempt + 1,
                    )

        return ProviderResult(
            success=False,
            error="no image in response after retries",
            attempts=retries,
        )


# ---------------------------------------------------------------------------
# Provider factory
# ---------------------------------------------------------------------------


def create_provider(
    provider_name: str = "gemini",
    model: str | None = None,
    api_key: str | None = None,
) -> ProviderAdapter:
    """Create a provider adapter by name.

    For testing, use 'fake' to get a FakeProvider with default config.
    """
    if provider_name == "fake":
        return FakeProvider()
    elif provider_name == "gemini":
        return GeminiProvider(model=model, api_key=api_key)
    else:
        raise ValueError(f"unknown provider: {provider_name}")
