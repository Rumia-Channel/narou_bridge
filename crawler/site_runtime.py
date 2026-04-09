from __future__ import annotations

import importlib
import logging
from dataclasses import dataclass, field
from types import ModuleType
from typing import Any, Dict, Iterable, Optional


DEFAULT_ACTIONS = frozenset(
    {"download", "login", "update", "re_download", "convert", "repair"}
)


@dataclass(frozen=True)
class SitePaths:
    folder_path: str
    data_path: str
    cookie_path: str


@dataclass(frozen=True)
class SiteBinding:
    site_key: str
    module_name: str
    module: ModuleType
    login_enabled: int
    interval: int
    paths: SitePaths


@dataclass(frozen=True)
class ActionContext:
    binding: SiteBinding
    key_data: str
    host_name: str

    @property
    def site_key(self) -> str:
        return self.binding.site_key

    @property
    def folder_path(self) -> str:
        return self.binding.paths.folder_path

    @property
    def data_path(self) -> str:
        return self.binding.paths.data_path

    @property
    def cookie_path(self) -> str:
        return self.binding.paths.cookie_path

    @property
    def login_enabled(self) -> int:
        return self.binding.login_enabled

    @property
    def interval(self) -> int:
        return self.binding.interval


@dataclass
class ActionResult:
    site: str
    action: str
    status: str
    message: str = ""
    payload: Dict[str, Any] = field(default_factory=dict)

    @property
    def ok(self) -> bool:
        return self.status in {"success", "skipped"}


@dataclass
class DispatchReport:
    action_name: str
    param: str
    results: list[ActionResult] = field(default_factory=list)

    @property
    def status_code(self) -> int:
        if not self.results:
            return 400
        if any(result.status == "success" for result in self.results):
            return 200
        if all(result.status == "skipped" for result in self.results):
            return 200
        return 400

    @property
    def succeeded(self) -> bool:
        return any(result.status == "success" for result in self.results)

    @property
    def failed(self) -> bool:
        return any(result.status == "failed" for result in self.results)

    def summary(self) -> str:
        if not self.results:
            return f"{self.action_name}:{self.param} -> no target site"
        parts = []
        for result in self.results:
            detail = result.message or result.status
            parts.append(f"{result.site}={detail}")
        return ", ".join(parts)


class BaseSite:
    allowed_actions = DEFAULT_ACTIONS

    def __init__(self):
        self.binding: Optional[SiteBinding] = None

    def bind(self, binding: SiteBinding) -> "BaseSite":
        self.binding = binding
        return self

    def supports(self, action_name: str) -> bool:
        return action_name in self.allowed_actions

    def matches_url(self, url: str) -> bool:
        return False

    def before_action(self, action_name: str, param: str, context: ActionContext):
        return None

    def execute(
        self, action_name: str, param: str, context: ActionContext
    ) -> ActionResult:
        if not self.supports(action_name):
            return ActionResult(context.site_key, action_name, "skipped", "unsupported")

        if action_name == "download" and not self.matches_url(param):
            return ActionResult(context.site_key, action_name, "skipped", "url mismatch")

        self.before_action(action_name, param, context)

        handler = getattr(self, f"on_{action_name}", None)
        if handler is None:
            return ActionResult(context.site_key, action_name, "skipped", "no handler")

        result = handler(param, context)
        if isinstance(result, ActionResult):
            return result
        if isinstance(result, str):
            return ActionResult(context.site_key, action_name, "success", result)
        return ActionResult(context.site_key, action_name, "success")


class LegacySiteAdapter(BaseSite):
    def __init__(self, module: ModuleType, module_name: str):
        super().__init__()
        self.module = module
        self.module_name = module_name
        self.allowed_actions = frozenset(
            getattr(module, "ALLOWED_ACTIONS", DEFAULT_ACTIONS)
        )

    def matches_url(self, url: str) -> bool:
        module_sig = self.module_name.replace("_", ".").replace(".py", "")
        return module_sig in url

    def _call_init(self, context: ActionContext):
        init_func = getattr(self.module, "init", None)
        if init_func is not None:
            init_func(
                context.cookie_path,
                context.data_path,
                int(context.login_enabled),
                context.interval,
            )

    def on_download(self, param: str, context: ActionContext):
        self._call_init(context)
        self.module.download(
            param,
            context.folder_path,
            context.key_data,
            context.data_path,
            context.host_name,
        )

    def on_login(self, param: str, context: ActionContext):
        account_name = None
        display_name = None
        if ":" in param:
            parts = param.split(":", 2)
            if len(parts) >= 2:
                account_name = parts[1]
            if len(parts) >= 3:
                display_name = parts[2]

        self.module.login(
            context.cookie_path,
            context.data_path,
            context.interval,
            account_name,
            display_name,
        )

    def on_update(self, _param: str, context: ActionContext):
        self._call_init(context)
        self.module.update(
            context.folder_path,
            context.key_data,
            context.data_path,
            context.host_name,
        )

    def on_re_download(self, _param: str, context: ActionContext):
        self._call_init(context)
        self.module.re_download(
            context.folder_path,
            context.key_data,
            context.data_path,
            context.host_name,
        )

    def on_convert(self, _param: str, context: ActionContext):
        self._call_init(context)
        self.module.convert(
            context.folder_path,
            context.key_data,
            context.data_path,
            context.host_name,
        )

    def on_repair(self, _param: str, context: ActionContext):
        self._call_init(context)
        self.module.repair(
            context.folder_path,
            context.key_data,
            context.data_path,
            context.host_name,
        )


class SiteRegistry:
    def __init__(self, sites: Dict[str, BaseSite]):
        self.sites = sites

    @classmethod
    def from_config(
        cls,
        site_dic: Dict[str, str],
        login_dic: Dict[str, int],
        folder_path: Dict[str, str],
        data_path: str,
        cookie_path: Dict[str, str],
        interval: int,
    ) -> "SiteRegistry":
        bound_sites: Dict[str, BaseSite] = {}

        for site_key, module_name in site_dic.items():
            module_import_name = "crawler." + module_name.replace(".py", "")
            module = importlib.import_module(module_import_name)
            site = _load_site(module, module_name)
            binding = SiteBinding(
                site_key=site_key,
                module_name=module_name,
                module=module,
                login_enabled=int(login_dic[site_key]),
                interval=int(interval),
                paths=SitePaths(
                    folder_path=folder_path[site_key],
                    data_path=data_path,
                    cookie_path=cookie_path[site_key],
                ),
            )
            bound_sites[site_key] = site.bind(binding)

        return cls(bound_sites)

    def resolve_targets(self, action_name: str, param: str) -> list[BaseSite]:
        if param == "all":
            return list(self.sites.values())

        if action_name != "download" and ":" in param:
            site_key = param.split(":", 1)[0]
            site = self.sites.get(site_key)
            return [site] if site else []

        if param in self.sites:
            return [self.sites[param]]

        if action_name == "download":
            for site in self.sites.values():
                if site.matches_url(param):
                    return [site]

        return []

    def dispatch(
        self, action_name: str, param: str, key_data: str, host_name: str
    ) -> DispatchReport:
        report = DispatchReport(action_name=action_name, param=param)
        targets = self.resolve_targets(action_name, param)
        if not targets:
            return report

        for site in targets:
            if site.binding is None:
                continue
            context = ActionContext(site.binding, key_data, host_name)
            try:
                result = site.execute(action_name, param, context)
            except Exception as exc:
                logging.error(
                    "Site action failed: site=%s action=%s param=%s error=%s",
                    context.site_key,
                    action_name,
                    param,
                    exc,
                    exc_info=True,
                )
                result = ActionResult(
                    context.site_key,
                    action_name,
                    "failed",
                    str(exc),
                )
            report.results.append(result)

        return report


def _load_site(module: ModuleType, module_name: str) -> BaseSite:
    factory = getattr(module, "create_site", None)
    if callable(factory):
        return factory()

    site = getattr(module, "SITE", None)
    if isinstance(site, type) and issubclass(site, BaseSite):
        return site()
    if isinstance(site, BaseSite):
        return site

    return LegacySiteAdapter(module, module_name)


def collect_failures(results: Iterable[ActionResult]) -> list[ActionResult]:
    return [result for result in results if result.status == "failed"]
