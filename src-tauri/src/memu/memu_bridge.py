#!/usr/bin/env python3
"""
memU Bridge Script for uClaw.

This script acts as a JSON-RPC over stdio bridge between the uClaw Rust backend
and the memU Python memory service. It reads JSON requests from stdin, dispatches
them to MemoryService, and writes JSON responses to stdout.

Protocol:
    Request:  {"id": <int>, "method": "<name>", "params": {...}}
    Response: {"id": <int>, "result": {...}}
    Error:    {"id": <int>, "error": {"message": "..."}}

Requires: Python 3.13+, memu package installed.

Environment variables:
    MEMU_DB_PATH   — SQLite database path (default: ~/.uclaw/memory/memu.db)
    MEMU_DATA_DIR  — uClaw data directory (default: ~/.uclaw)
"""

from __future__ import annotations

import asyncio
import json
import os
import sys
import traceback
from pathlib import Path
from typing import Any

# ─── Dependency Check ────────────────────────────────────────────────────

def check_memu_available() -> bool:
    """Check if the memu package is importable."""
    try:
        import memu  # noqa: F401
        return True
    except ImportError:
        return False


MEMU_AVAILABLE = check_memu_available()

if not MEMU_AVAILABLE:
    memu_local = Path.home() / "Documents" / "memU"
    if memu_local.exists():
        sys.stderr.write(
            f"[memu_bridge] memu package not found. "
            f"Please install it: pip install -e {memu_local}\n"
        )
    else:
        sys.stderr.write(
            "[memu_bridge] memu package not found. "
            "Please install it: pip install memu\n"
        )
    sys.stderr.write("[memu_bridge] Running in DEGRADED mode — all requests will return errors.\n")
    MemoryService = None  # type: ignore[assignment,misc]
else:
    from memu.app import MemoryService  # noqa: E402

# ─── FastEmbed (local embedding) ─────────────────────────────────────────

FASTEMBED_AVAILABLE = False
_fastembed_model = None
try:
    from fastembed import TextEmbedding  # type: ignore[import-untyped]
    FASTEMBED_AVAILABLE = True
except ImportError:
    pass


def _get_fastembed_model(model_name: str = "BAAI/bge-small-en-v1.5"):
    """Lazily initialize and cache the FastEmbed model."""
    global _fastembed_model
    if _fastembed_model is None:
        _fastembed_model = TextEmbedding(model_name=model_name)
        sys.stderr.write(f"[memu_bridge] FastEmbed model loaded: {model_name}\n")
    return _fastembed_model

# ─── Reasoning Model Mapping ────────────────────────────────────────────

# ─── Deduplication Helpers ────────────────────────────────────────────────

def _text_similarity(a: str, b: str) -> float:
    """Jaccard similarity on word/character sets with CJK support."""
    # 对中文文本按字符切分，对英文按词切分
    def tokenize(text: str) -> set[str]:
        tokens = set()
        current_word = []
        for char in text.lower():
            if '\u4e00' <= char <= '\u9fff':  # CJK character
                if current_word:
                    word = ''.join(current_word)
                    if len(word) > 1:
                        tokens.add(word)
                    current_word = []
                tokens.add(char)
            elif char.isalnum():
                current_word.append(char)
            else:
                if current_word:
                    word = ''.join(current_word)
                    if len(word) > 1:
                        tokens.add(word)
                    current_word = []
        if current_word:
            word = ''.join(current_word)
            if len(word) > 1:
                tokens.add(word)
        return tokens

    tokens_a = tokenize(a)
    tokens_b = tokenize(b)
    if not tokens_a and not tokens_b:
        return 1.0
    intersection = len(tokens_a & tokens_b)
    union = len(tokens_a | tokens_b)
    return intersection / union if union > 0 else 0.0


def _categories_from_result(result: dict) -> list[str]:
    """Extract category names that were actually updated in this memorize call.

    Uses the ``relations`` list (item_id → category_id) together with the
    ``categories`` list to resolve names.  Falls back to returning all categories
    that have a non-null summary if relations data is unavailable.
    """
    categories = result.get("categories", [])
    relations = result.get("relations", [])

    if relations and categories:
        # Build a map category_id → name
        cat_id_to_name: dict[str, str] = {}
        for cat in categories:
            cid = cat.get("id")
            name = cat.get("name")
            if cid and name:
                cat_id_to_name[cid] = name

        # Collect the names referenced by new relations
        updated: list[str] = []
        seen: set[str] = set()
        for rel in relations:
            cid = rel.get("category_id")
            if cid and cid in cat_id_to_name:
                name = cat_id_to_name[cid]
                if name not in seen:
                    seen.add(name)
                    updated.append(name)
        return updated

    # Fallback: any category with a summary (conservative)
    return [cat["name"] for cat in categories if isinstance(cat, dict) and cat.get("name") and cat.get("summary")]


def _deduplicate_items(items: list[dict]) -> list[dict]:
    """Remove semantically similar memory items, keeping the more detailed one."""
    if not items:
        return items

    result: list[dict] = []
    for item in items:
        summary = item.get("summary", item.get("content", ""))

        is_duplicate = False
        for i, existing in enumerate(result):
            existing_summary = existing.get("summary", existing.get("content", ""))
            if _text_similarity(summary, existing_summary) > 0.5:
                # Keep the longer/more detailed one
                if len(summary) > len(existing_summary):
                    result[i] = item
                is_duplicate = True
                break

        if not is_duplicate:
            result.append(item)

    return result


# ─── Reasoning Model Mapping (continued) ─────────────────────────────────

# DeepSeek reasoning models output to `reasoning_content` instead of `content`,
# which is incompatible with memU's LLM client that reads `message.content`.
# Map them to non-reasoning equivalents for the memorize workflow.
REASONING_MODEL_MAP: dict[str, str] = {
    "deepseek-v4-flash": "deepseek-chat",
    "deepseek-reasoner": "deepseek-chat",
    "deepseek-r1": "deepseek-chat",
    "deepseek-r1-lite": "deepseek-chat",
}


def _map_to_non_reasoning_model(model_id: str) -> str:
    """Map reasoning model IDs to their non-reasoning equivalents."""
    mapped = REASONING_MODEL_MAP.get(model_id)
    if mapped:
        sys.stderr.write(
            f"[memu_bridge] Mapped reasoning model '{model_id}' -> '{mapped}' "
            f"(reasoning models output to reasoning_content, not content)\n"
        )
        return mapped
    return model_id


# ─── Configuration ───────────────────────────────────────────────────────

# uClaw custom 9 memory categories
UCLAW_CATEGORIES = [
    {
        "name": "Boot",
        "description": "System bootstrap memories: initial setup, first-run context, and onboarding information.",
    },
    {
        "name": "Identity",
        "description": "Core identity attributes: name, age, location, profession, language, and personal identifiers.",
    },
    {
        "name": "Value",
        "description": "User values, beliefs, principles, and ethical standpoints that guide decision-making.",
    },
    {
        "name": "UserProfile",
        "description": "Behavioral patterns, communication style, preferences, and personality traits.",
    },
    {
        "name": "Directive",
        "description": "Explicit instructions, rules, and constraints the user has set for the AI assistant.",
    },
    {
        "name": "Curated",
        "description": "Manually curated knowledge: bookmarks, saved facts, reference materials, and pinned information.",
    },
    {
        "name": "Episode",
        "description": "Episodic memories: notable conversations, events, interactions, and temporal experiences.",
    },
    {
        "name": "Procedure",
        "description": "Learned procedures, workflows, step-by-step processes, and operational knowledge.",
    },
    {
        "name": "Reference",
        "description": "External references: links, documents, tools, APIs, and technical resources.",
    },
]


def build_service():
    """Initialize the MemoryService with uClaw-specific configuration.

    Returns None if the memu package is not available (degraded mode).
    """
    if not MEMU_AVAILABLE:
        return None

    data_dir = Path(os.environ.get("MEMU_DATA_DIR", str(Path.home() / ".uclaw")))
    db_path = Path(os.environ.get("MEMU_DB_PATH", str(data_dir / "memory" / "memu.db")))

    # Ensure parent directories exist
    db_path.parent.mkdir(parents=True, exist_ok=True)

    # Build database config pointing to SQLite
    database_config = {
        "metadata_store": {
            "provider": "sqlite",
            "dsn": f"sqlite:///{db_path}",
        },
    }

    # Build memorize config with uClaw categories
    memorize_config = {
        "memory_categories": UCLAW_CATEGORIES,
    }

    # LLM profiles — read from environment or use defaults
    # The actual LLM config will be injected by the Rust side via env vars
    api_key = os.environ.get("MEMU_LLM_API_KEY", os.environ.get("OPENAI_API_KEY", ""))
    base_url = os.environ.get("MEMU_LLM_BASE_URL", "https://api.openai.com/v1")
    chat_model = os.environ.get("MEMU_LLM_CHAT_MODEL", "gpt-4o-mini")

    # Fallback: read from providers.json if env vars didn't provide an API key
    if not api_key:
        providers_path = data_dir / "providers.json"
        if providers_path.exists():
            try:
                import json as _json
                with open(providers_path) as f:
                    config = _json.load(f)
                active = config.get("active_model", {})
                provider_id = active.get("provider_id", "")
                model_id = active.get("model_id", "")
                for p in config.get("providers", []):
                    if p.get("provider_id") == provider_id:
                        api_key = p.get("api_key", "")
                        prov_base_url = p.get("base_url", "")
                        if prov_base_url:
                            base_url = prov_base_url
                        if model_id:
                            chat_model = model_id
                        break
                if api_key:
                    sys.stderr.write(f"[memu_bridge] Loaded API key from providers.json (provider: {provider_id})\n")
            except Exception as e:
                sys.stderr.write(f"[memu_bridge] Failed to load providers.json: {e}\n")

    # Map reasoning models to non-reasoning equivalents.
    # Reasoning models (e.g. deepseek-v4-flash, deepseek-reasoner) return output
    # in reasoning_content instead of content, which memU's LLM client cannot parse.
    chat_model = _map_to_non_reasoning_model(chat_model)
    embed_model = os.environ.get("MEMU_LLM_EMBED_MODEL", "text-embedding-3-small")

    llm_profiles = {
        "default": {
            "api_key": api_key,
            "base_url": base_url,
            "chat_model": chat_model,
            "embed_model": embed_model,
            "client_backend": "sdk",
        },
        "embedding": {
            "api_key": api_key,
            "base_url": base_url,
            "chat_model": chat_model,
            "embed_model": embed_model,
            "client_backend": "sdk",
        },
    }

    sys.stderr.write(
        f"[memu_bridge] LLM config: model={chat_model}, base_url={base_url}, "
        f"api_key={'set' if api_key else 'EMPTY'}\n"
    )

    # Resources directory for blob storage
    blob_config = {
        "resources_dir": str(data_dir / "memory" / "resources"),
    }

    service = MemoryService(
        llm_profiles=llm_profiles,
        database_config=database_config,
        memorize_config=memorize_config,
        blob_config=blob_config,
    )

    # ── FastEmbed monkey-patch ────────────────────────────────────────
    embed_mode = os.environ.get("MEMU_EMBED_MODE", "auto")

    use_fastembed = False
    if embed_mode == "fastembed" and FASTEMBED_AVAILABLE:
        use_fastembed = True
    elif embed_mode == "auto" and FASTEMBED_AVAILABLE:
        use_fastembed = True

    if use_fastembed:
        fastembed_model_name = os.environ.get("FASTEMBED_MODEL", "BAAI/bge-small-en-v1.5")
        fe_model = _get_fastembed_model(fastembed_model_name)

        async def _fastembed_embed(inputs: list[str], **kwargs) -> tuple[list[list[float]], None]:
            """Local FastEmbed embedding — drop-in replacement for OpenAISDKClient.embed()."""
            embeddings = list(fe_model.embed(inputs))
            return [emb.tolist() for emb in embeddings], None

        # Patch _init_llm_client so every lazily-created client uses FastEmbed
        _original_init_llm_client = service._init_llm_client

        def _patched_init_llm_client(config=None):
            client = _original_init_llm_client(config)
            client.embed = _fastembed_embed
            return client

        service._init_llm_client = _patched_init_llm_client  # type: ignore[method-assign]
        sys.stderr.write(f"[memu_bridge] Using FastEmbed for embeddings: {fastembed_model_name}\n")
    elif embed_mode in ("fastembed", "auto") and not FASTEMBED_AVAILABLE:
        sys.stderr.write("[memu_bridge] FastEmbed not available, falling back to OpenAI embedding\n")

    return service


# ─── Request Handlers ────────────────────────────────────────────────────

async def handle_health(service: MemoryService, params: Any) -> dict[str, Any]:
    """Health check handler."""
    return {"status": "ok"}


async def handle_ping(service: MemoryService, params: Any) -> dict[str, Any]:
    """Ping handler for startup health check and heartbeat."""
    return {"status": "pong"}


async def handle_memorize(service: MemoryService, params: dict[str, Any]) -> dict[str, Any]:
    """Handle a memorize request.

    The Rust side sends raw text content in ``params["content"]``, but memU's
    ``memorize()`` expects ``resource_url`` to be a valid file path or HTTP URL
    (it is passed to ``LocalFS.fetch()``).  When the content is plain text (not
    a path / URL), we write it to a temporary file first and pass the file path.
    """
    content = params.get("content", "")
    modality = params.get("modality", "text")
    user_scope = params.get("user_scope")

    resource_url = _resolve_resource_url(content, modality)

    kwargs: dict[str, Any] = {
        "resource_url": resource_url,
        "modality": modality,
    }
    if user_scope:
        kwargs["user"] = user_scope

    result = await service.memorize(**kwargs)

    # Deduplicate items before returning to Rust side
    if "items" in result and isinstance(result["items"], list):
        original_count = len(result["items"])
        result["items"] = _deduplicate_items(result["items"])
        deduped_count = len(result["items"])
        if original_count != deduped_count:
            sys.stderr.write(
                f"[memu_bridge] Deduplicated items: {original_count} -> {deduped_count}\n"
            )

    # Debug logging for memorize results
    items = result.get("items", [])
    sys.stderr.write(
        f"[memu_bridge] memorize completed: {len(items)} items extracted, "
        f"modality={modality}, resource_url={resource_url[:80]}\n"
    )
    for i, item in enumerate(items[:5]):
        summary = (item.get("summary") or "")[:80]
        sys.stderr.write(f"[memu_bridge]   item[{i}]: {summary}\n")
    if len(items) > 5:
        sys.stderr.write(f"[memu_bridge]   ... and {len(items) - 5} more items\n")

    return result


def _is_url_or_path(value: str) -> bool:
    """Return True if *value* looks like a file path or HTTP(S) URL."""
    stripped = value.strip()
    if not stripped:
        return False
    # Long strings or multi-line content are never paths/URLs
    if len(stripped) > 1000 or '\n' in stripped or '\r' in stripped:
        return False
    # HTTP / HTTPS URL
    if stripped.startswith(("http://", "https://")):
        return True
    # Absolute or home-relative file path
    if stripped.startswith(("/", "~")):
        try:
            return Path(os.path.expanduser(stripped)).exists()
        except OSError:
            return False
    # Relative path that exists on disk
    try:
        p = Path(stripped)
        if p.exists():
            return True
    except OSError:
        return False
    return False


def _resolve_resource_url(content: str, modality: str) -> str:
    """Ensure *content* is a valid resource URL for memU.

    If the content is already a URL or an existing file path it is returned
    as-is.  Otherwise the text is persisted to a temporary file under
    ``~/.uclaw/memory/resources/`` and the file path is returned.
    """
    if _is_url_or_path(content):
        return content

    # Write text content to a temporary file
    import hashlib
    import time

    data_dir = Path(os.environ.get("MEMU_DATA_DIR", str(Path.home() / ".uclaw")))
    resources_dir = data_dir / "memory" / "resources"
    resources_dir.mkdir(parents=True, exist_ok=True)

    ext_map = {"conversation": "txt", "text": "txt", "document": "txt"}
    ext = ext_map.get(modality, "txt")
    # Use a hash prefix to avoid filename collisions while staying readable
    content_hash = hashlib.sha256(content.encode("utf-8")).hexdigest()[:12]
    ts = int(time.time())
    filename = f"memorize_{ts}_{content_hash}.{ext}"
    tmp_path = resources_dir / filename
    tmp_path.write_text(content, encoding="utf-8")
    sys.stderr.write(f"[memu_bridge] Wrote inline content to temp file: {tmp_path}\n")
    return str(tmp_path)


async def handle_retrieve(service: MemoryService, params: dict[str, Any]) -> dict[str, Any]:
    """Handle a retrieve request."""
    queries = params.get("queries", [])
    where_clause = params.get("where")
    user_scope = params.get("user_scope")

    kwargs: dict[str, Any] = {"queries": queries}
    if where_clause:
        kwargs["where"] = where_clause
    if user_scope:
        # If the service supports user scoping in retrieve, pass it
        pass

    result = await service.retrieve(**kwargs)
    return result


async def handle_create_item(service: MemoryService, params: dict[str, Any]) -> dict[str, Any]:
    """Handle a create_item request."""
    memory_type = params.get("memory_type", "knowledge")
    memory_content = params.get("memory_content", "")
    memory_categories = params.get("memory_categories", [])
    user_scope = params.get("user_scope")

    kwargs: dict[str, Any] = {
        "memory_type": memory_type,
        "memory_content": memory_content,
        "memory_categories": memory_categories,
    }
    if user_scope:
        kwargs["user"] = user_scope

    result = await service.create_memory_item(**kwargs)
    return result


async def handle_delete_item(service: MemoryService, params: dict[str, Any]) -> dict[str, Any]:
    """Handle a delete_item request."""
    memory_id = params.get("id", "")
    user_scope = params.get("user_scope")

    kwargs: dict[str, Any] = {"memory_id": memory_id}
    if user_scope:
        kwargs["user"] = user_scope

    result = await service.delete_memory_item(**kwargs)
    return result or {}


async def handle_list_items(service: MemoryService, params: dict[str, Any]) -> dict[str, Any]:
    """Handle a list_items request."""
    where_clause = None
    user_scope = params.get("user_scope")

    # Build where clause from filters
    filters: dict[str, Any] = {}
    if params.get("category"):
        filters["category"] = params["category"]
    if params.get("memory_type"):
        filters["memory_type"] = params["memory_type"]
    if user_scope:
        filters.update(user_scope)
    if filters:
        where_clause = filters

    result = await service.list_memory_items(where=where_clause)
    return result


async def handle_list_categories(service: MemoryService, params: dict[str, Any]) -> dict[str, Any]:
    """Handle a list_categories request."""
    user_scope = params.get("user_scope")
    where_clause = user_scope if user_scope else None

    result = await service.list_memory_categories(where=where_clause)
    return result


async def handle_memorize_with_config(service: MemoryService, params: dict[str, Any]) -> dict[str, Any]:
    """Handle a memorize_with_config request (used by ProactiveService).

    Params (wrapped in ``input`` key):
        content      — raw text to memorize
        memory_types — hint list (e.g. ["profile", "behavior"]); used only for
                       result reporting, not passed to MemoryService
        categories   — optional category filter (currently informational)
        source_type  — caller label e.g. "proactive_conversation_learning"

    Returns a ScenarioMemorizeResult-shaped dict:
        {"items_extracted": N, "categories_updated": [...], "source_type": str}
    """
    inp = params.get("input", params)  # support both wrapped and flat layouts
    content = inp.get("content", "")
    source_type = inp.get("source_type", "unknown")
    # memory_types and categories are informational hints — MemoryService decides
    # what types to extract on its own based on content.

    if not content.strip():
        return {"items_extracted": 0, "categories_updated": [], "source_type": source_type}

    resource_url = _resolve_resource_url(content, "conversation")

    try:
        result = await service.memorize(resource_url=resource_url, modality="text")
    except Exception as e:
        sys.stderr.write(f"[memu_bridge] memorize_with_config error: {e}\n")
        raise

    # Deduplicate
    items = _deduplicate_items(result.get("items", []))
    categories_updated = _categories_from_result(result)

    sys.stderr.write(
        f"[memu_bridge] memorize_with_config completed: {len(items)} items, "
        f"source_type={source_type}, categories={categories_updated}\n"
    )

    return {
        "items_extracted": len(items),
        "categories_updated": categories_updated,
        "source_type": source_type,
    }


async def handle_retrieve_with_context(service: MemoryService, params: dict[str, Any]) -> dict[str, Any]:
    """Handle a retrieve_with_context request.

    Params (wrapped in ``input`` key):
        query             — search query string
        memory_types      — optional list of type filters (currently informational)
        limit             — max items to return (default 10)
        include_categories — whether to include category info (always included)

    Returns {"items": [EnrichedMemoryItem, ...]}.
    """
    inp = params.get("input", params)
    query = inp.get("query", "")
    limit = int(inp.get("limit", 10))

    if not query.strip():
        return {"items": []}

    try:
        result = await service.retrieve(queries=[query])
    except Exception as e:
        sys.stderr.write(f"[memu_bridge] retrieve_with_context error: {e}\n")
        raise

    raw_items = result.get("items", [])[:limit]

    # Map to EnrichedMemoryItem shape expected by the Rust side
    enriched: list[dict[str, Any]] = []
    for item in raw_items:
        enriched.append({
            "content": item.get("summary") or item.get("memory_content") or "",
            "memory_type": item.get("memory_type", "knowledge"),
            "relevance_score": float(item.get("score", item.get("relevance_score", 0.0))),
            "categories": item.get("categories", []),
            "metadata": item.get("extra") or item.get("metadata") or {},
            "created_at": item.get("created_at"),
        })

    return {"items": enriched}


async def handle_embed_text(service: Any, params: dict[str, Any]) -> dict[str, Any]:
    """Embed a list of texts using the local FastEmbed model.

    Params:
        texts — list of strings to embed

    Returns:
        {"vectors": [[f32, ...], ...]}  — one vector per input text, or
        {"error": "..."} if FastEmbed is unavailable.
    """
    texts = params.get("texts", [])
    if not texts:
        return {"vectors": []}

    if not FASTEMBED_AVAILABLE:
        raise RuntimeError("FastEmbed is not available (pip install fastembed)")

    model = _get_fastembed_model()
    embeddings = list(model.embed(texts))
    vectors = [emb.tolist() for emb in embeddings]
    return {"vectors": vectors}


async def handle_memorize_multimodal(service: MemoryService, params: dict[str, Any]) -> dict[str, Any]:
    """Handle a memorize_multimodal request.

    The Rust side pre-combines text + caption into:
        "[Caption: {caption}]\n\n{text}"
    and includes source_type and metadata.

    Params (wrapped in ``input`` key):
        content     — pre-combined multimodal text
        source_type — "image" | "document" | "code" | "audio"
        metadata    — additional metadata dict

    Returns a ScenarioMemorizeResult-shaped dict.
    """
    inp = params.get("input", params)
    content = inp.get("content", "")
    source_type = inp.get("source_type", "multimodal")

    if not content.strip():
        return {"items_extracted": 0, "categories_updated": [], "source_type": source_type}

    resource_url = _resolve_resource_url(content, "text")

    try:
        result = await service.memorize(resource_url=resource_url, modality="text")
    except Exception as e:
        sys.stderr.write(f"[memu_bridge] memorize_multimodal error: {e}\n")
        raise

    items = _deduplicate_items(result.get("items", []))
    categories_updated = _categories_from_result(result)

    sys.stderr.write(
        f"[memu_bridge] memorize_multimodal completed: {len(items)} items, source_type={source_type}\n"
    )

    return {
        "items_extracted": len(items),
        "categories_updated": categories_updated,
        "source_type": source_type,
    }


# Method dispatch table
HANDLERS: dict[str, Any] = {
    "health": handle_health,
    "ping": handle_ping,
    "memorize": handle_memorize,
    "memorize_with_config": handle_memorize_with_config,
    "memorize_multimodal": handle_memorize_multimodal,
    "retrieve": handle_retrieve,
    "retrieve_with_context": handle_retrieve_with_context,
    "create_item": handle_create_item,
    "delete_item": handle_delete_item,
    "list_items": handle_list_items,
    "list_categories": handle_list_categories,
    "embed_text": handle_embed_text,
}


# ─── Main Loop ───────────────────────────────────────────────────────────

async def process_request(service, request: dict[str, Any]) -> dict[str, Any]:
    """Process a single JSON-RPC request and return a response."""
    req_id = request.get("id", 0)
    method = request.get("method", "")
    params = request.get("params", {})

    # Health check always works, even in degraded mode
    if method == "health":
        status = "ok" if service is not None else "degraded"
        return {"id": req_id, "result": {"status": status}}

    handler = HANDLERS.get(method)
    if handler is None:
        return {
            "id": req_id,
            "error": {"message": f"Unknown method: {method}"},
        }

    # embed_text only needs FastEmbed, not the memu service — allow in degraded mode.
    # All other methods require a live memu service.
    if service is None and method != "embed_text":
        return {
            "id": req_id,
            "error": {"message": "memU service unavailable (package not installed)"},
        }

    try:
        result = await handler(service, params if params else {})
        return {"id": req_id, "result": result}
    except Exception as e:
        tb = traceback.format_exc()
        sys.stderr.write(f"[memu_bridge] Error in method '{method}': {tb}\n")
        return {
            "id": req_id,
            "error": {"message": str(e)},
        }


async def main() -> None:
    """Main event loop: read JSON from stdin, process, write JSON to stdout."""
    sys.stderr.write("[memu_bridge] Initializing memU service...\n")

    try:
        service = build_service()
    except Exception as e:
        tb = traceback.format_exc()
        sys.stderr.write(f"[memu_bridge] Failed to initialize service: {tb}\n")
        # Continue in degraded mode instead of exiting — let Rust side get
        # proper error responses rather than a cryptic "RequestCancelled".
        service = None

    if service is not None:
        sys.stderr.write("[memu_bridge] Service initialized. Listening for requests on stdin...\n")
    else:
        sys.stderr.write("[memu_bridge] Running in DEGRADED mode. Listening for requests on stdin...\n")

    # Use a robust stdin reading approach that works in subprocess (pipe) mode
    loop = asyncio.get_event_loop()

    # 32 MB line limit — large memorization payloads (long documents, code files)
    # easily exceed the default 64 KB limit and raise LimitOverrunError.
    _LINE_LIMIT = 32 * 1024 * 1024

    # Always use thread-based stdin reading --- more reliable than
    # connect_read_pipe on macOS with Python 3.13, where kqueue can
    # fail with EINVAL for certain pipe fd types.
    reader = asyncio.StreamReader(limit=_LINE_LIMIT)

    def _stdin_reader():
        try:
            while True:
                line = sys.stdin.buffer.readline()
                if not line:
                    loop.call_soon_threadsafe(reader.feed_eof)
                    break
                loop.call_soon_threadsafe(reader.feed_data, line)
        except Exception:
            loop.call_soon_threadsafe(reader.feed_eof)

    import threading
    t = threading.Thread(target=_stdin_reader, daemon=True)
    t.start()
    sys.stderr.write("[memu_bridge] stdin reader thread started\n")

    while True:
        try:
            line = await reader.readline()
            if not line:
                # EOF — stdin closed, exit gracefully
                sys.stderr.write("[memu_bridge] stdin closed, shutting down.\n")
                break

            line_str = line.decode("utf-8").strip()
            if not line_str:
                continue

            try:
                request = json.loads(line_str)
            except json.JSONDecodeError as e:
                sys.stderr.write(f"[memu_bridge] Invalid JSON: {e}\n")
                error_response = json.dumps({
                    "id": 0,
                    "error": {"message": f"Invalid JSON: {e}"},
                })
                sys.stdout.write(error_response + "\n")
                sys.stdout.flush()
                continue

            response = await process_request(service, request)
            response_line = json.dumps(response, ensure_ascii=False, default=str)
            sys.stdout.write(response_line + "\n")
            sys.stdout.flush()

        except Exception as e:
            sys.stderr.write(f"[memu_bridge] Unexpected error: {traceback.format_exc()}\n")
            # Try to write an error response
            try:
                error_response = json.dumps({
                    "id": 0,
                    "error": {"message": f"Internal bridge error: {e}"},
                })
                sys.stdout.write(error_response + "\n")
                sys.stdout.flush()
            except Exception:
                pass


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        sys.stderr.write("[memu_bridge] Interrupted, exiting.\n")
    except Exception as e:
        sys.stderr.write(f"[memu_bridge] Fatal error: {traceback.format_exc()}\n")
        sys.exit(1)
