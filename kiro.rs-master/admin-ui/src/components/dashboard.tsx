import { useState, useEffect, useRef } from "react";
import {
  RefreshCw,
  LogOut,
  Moon,
  Sun,
  Server,
  Plus,
  Upload,
  FileUp,
  FileDown,
  Trash2,
  RotateCcw,
  CheckCircle2,
  Globe,
  LogIn,
  Key,
  Building2,
  Settings,
  UploadCloud,
  MoreHorizontal,
  Activity,
  ChevronLeft,
  ChevronRight,
  AlertTriangle,
  Eye,
  EyeOff,
  Copy,
  Wand2,
  Zap,
  Tags,
  ChevronDown,
  LayoutGrid,
  List,
  Search,
  X,
} from "lucide-react";

function GithubIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="currentColor"
      className={className}
      aria-hidden="true"
    >
      <path d="M12 .5C5.65.5.5 5.65.5 12.02c0 5.1 3.29 9.42 7.86 10.95.58.11.79-.25.79-.55 0-.27-.01-.99-.02-1.95-3.2.7-3.87-1.54-3.87-1.54-.52-1.32-1.27-1.67-1.27-1.67-1.04-.71.08-.7.08-.7 1.15.08 1.76 1.18 1.76 1.18 1.02 1.76 2.69 1.25 3.34.95.1-.74.4-1.25.72-1.54-2.55-.29-5.24-1.28-5.24-5.69 0-1.26.45-2.29 1.18-3.09-.12-.29-.51-1.46.11-3.05 0 0 .96-.31 3.16 1.18a10.95 10.95 0 0 1 5.75 0c2.2-1.49 3.16-1.18 3.16-1.18.62 1.59.23 2.76.12 3.05.74.8 1.18 1.83 1.18 3.09 0 4.42-2.69 5.39-5.26 5.68.41.36.78 1.06.78 2.14 0 1.55-.01 2.79-.01 3.17 0 .31.21.67.8.55A11.51 11.51 0 0 0 23.5 12.02C23.5 5.65 18.35.5 12 .5Z" />
    </svg>
  );
}
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { storage, type CredentialView } from "@/lib/storage";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
  DialogDescription,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";
import { CredentialCard } from "@/components/credential-card";
import { AddCredentialDialog } from "@/components/add-credential-dialog";
import { BatchImportDialog } from "@/components/batch-import-dialog";
import { BatchEditCredentialDialog } from "@/components/batch-edit-credential-dialog";
import { IdcLoginDialog } from "@/components/idc-login-dialog";
import { SocialLoginDialog } from "@/components/social-login-dialog";
import { KamImportDialog } from "@/components/kam-import-dialog";
import {
  BatchVerifyDialog,
  type VerifyResult,
} from "@/components/batch-verify-dialog";
import { detectTier, type Tier } from "@/components/subscription-badge";
import { ProxyPoolDialog } from "@/components/proxy-pool-dialog";
import { ImageUpdateDialog } from "@/components/image-update-dialog";
import { useConfirm } from "@/components/ui/confirm-dialog";
import {
  useCredentials,
  useDeleteCredential,
  useResetFailure,
  useLoadBalancingMode,
  useSetLoadBalancingMode,
  useResetAllSuccessCount,
  useSetPriority,
} from "@/hooks/use-credentials";
import { useUpdateCheck } from "@/hooks/use-update-check";
import { useFailureStats } from "@/hooks/use-traces";
import { useGroupOptions } from "@/hooks/use-groups";
import { useRectSelect } from "@/hooks/use-rect-select";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Checkbox } from "@/components/ui/checkbox";
import {
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  closestCenter,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  rectSortingStrategy,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import {
  getCredentialBalance,
  forceRefreshToken,
  disableQuotaExceeded,
  enableOverageForAllCapable,
  exportKamCredentials,
  updateAdminKey,
} from "@/api/credentials";
import {
  extractErrorMessage,
  parseError,
  generateApiKey,
  formatNumber,
  overageFailureMessage,
} from "@/lib/utils";
import type { BalanceResponse } from "@/types/api";

interface DashboardProps {
  onLogout: () => void;
  /** treat asas Tab embed into App when insideas true:Hidebuilt-intop bar and outer layout,byparent App provide */
  embedded?: boolean;
}

// Subscription tierfilterofoptional item(key and detectTier Backvalue matches)
const TIER_OPTIONS: { value: Tier; label: string }[] = [
  { value: "free", label: "FREE" },
  { value: "pro", label: "PRO" },
  { value: "pro_plus", label: "PRO+" },
  { value: "power", label: "POWER" },
  { value: "unknown", label: "Unknown/Not queried" },
];
const TIER_LABELS: Record<Tier, string> = {
  free: "FREE",
  pro: "PRO",
  pro_plus: "PRO+",
  power: "POWER",
  unknown: "Unknown",
};

// Per pageCountoptional item;anotherhas“All”(pageSize = 0)bydropdown separatelyAppend
const PAGE_SIZE_OPTIONS = [12, 24, 48, 96] as const;

export function Dashboard({ onLogout, embedded = false }: DashboardProps) {
  const confirm = useConfirm();
  const [addDialogOpen, setAddDialogOpen] = useState(false);
  const [batchImportDialogOpen, setBatchImportDialogOpen] = useState(false);
  const [batchEditDialogOpen, setBatchEditDialogOpen] = useState(false);
  const [idcLoginDialogOpen, setIdcLoginDialogOpen] = useState(false);
  const [enterpriseLoginDialogOpen, setEnterpriseLoginDialogOpen] =
    useState(false);
  const [socialLoginDialogOpen, setSocialLoginDialogOpen] = useState(false);
  const [kamImportDialogOpen, setKamImportDialogOpen] = useState(false);
  const [proxyPoolDialogOpen, setProxyPoolDialogOpen] = useState(false);
  const [imageUpdateDialogOpen, setImageUpdateDialogOpen] = useState(false);
  const [adminKeyDialogOpen, setAdminKeyDialogOpen] = useState(false);
  const [newAdminKey, setNewAdminKey] = useState("");
  const [updatingAdminKey, setUpdatingAdminKey] = useState(false);
  const [showAdminKeyPlain, setShowAdminKeyPlain] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [verifyDialogOpen, setVerifyDialogOpen] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [verifyDeleting, setVerifyDeleting] = useState(false);
  const [verifyProgress, setVerifyProgress] = useState({
    current: 0,
    total: 0,
  });
  const [verifyResults, setVerifyResults] = useState<Map<number, VerifyResult>>(
    new Map(),
  );
  const [balanceMap, setBalanceMap] = useState<Map<number, BalanceResponse>>(
    new Map(),
  );
  const [loadingBalanceIds, setLoadingBalanceIds] = useState<Set<number>>(
    new Set(),
  );
  const [queryingInfo, setQueryingInfo] = useState(false);
  const [queryInfoProgress, setQueryInfoProgress] = useState({
    current: 0,
    total: 0,
  });
  const [batchRefreshing, setBatchRefreshing] = useState(false);
  const [batchRefreshProgress, setBatchRefreshProgress] = useState({
    current: 0,
    total: 0,
  });
  const cancelVerifyRef = useRef(false);
  const [currentPage, setCurrentPage] = useState(1);
  // display form(Card / List)andPer pageCount,all persisted to localStorage
  const [viewMode, setViewMode] = useState<CredentialView>(() =>
    storage.getCredentialView(),
  );
  const [pageSize, setPageSize] = useState<number>(() =>
    storage.getCredentialPageSize(),
  );
  const changeViewMode = (v: CredentialView) => {
    setViewMode(v);
    storage.setCredentialView(v);
  };
  const changePageSize = (n: number) => {
    setPageSize(n);
    storage.setCredentialPageSize(n);
    setCurrentPage(1);
  };
  const [darkMode, setDarkMode] = useState(() => {
    if (typeof window !== "undefined") {
      return document.documentElement.classList.contains("dark");
    }
    return false;
  });

  const queryClient = useQueryClient();
  const { data, isLoading, error, refetch } = useCredentials();
  const { mutate: deleteCredential } = useDeleteCredential();
  const { mutate: resetFailure } = useResetFailure();
  const { data: loadBalancingData, isLoading: isLoadingMode } =
    useLoadBalancingMode();
  const { mutate: setLoadBalancingMode, isPending: isSettingMode } =
    useSetLoadBalancingMode();
  const resetAllSuccess = useResetAllSuccessCount();
  const setPriority = useSetPriority();
  const { data: updateCheck } = useUpdateCheck();
  const { data: failureStatsMap } = useFailureStats();
  const groupOptions = useGroupOptions();

  // GroupFilter:'' = All;'__none__' = onlyShowUngrouped;other = byGroup namefilter
  const [groupFilter, setGroupFilter] = useState<string>("");
  // Subscription tierfilter(multi select):empty set = All tiers;otherwise onlyShowinside the setofTier
  const [tierFilter, setTierFilter] = useState<Set<Tier>>(new Set());
  // fuzzy search:bySource channel(Note)/ EmaildoSizecase insensitiveofsubstring match;empty string = no limit
  const [searchQuery, setSearchQuery] = useState("");
  const toggleTier = (t: Tier) => {
    setTierFilter((prev) => {
      const next = new Set(prev);
      if (next.has(t)) next.delete(t);
      else next.add(t);
      return next;
    });
  };

  // ApplyGroup + Tierafter filteringofCredentialfull set(splitpagefilter first before,ensure pagingpagecorrect granularity)
  const filteredCredentials = (() => {
    const all = data?.credentials ?? [];
    let out = all;
    if (groupFilter) {
      out =
        groupFilter === "__none__"
          ? out.filter((c) => !c.groups || c.groups.length === 0)
          : out.filter((c) => c.groups?.includes(groupFilter));
    }
    if (tierFilter.size > 0) {
      out = out.filter((c) =>
        tierFilter.has(detectTier(c.balance?.subscriptionTitle)),
      );
    }
    const q = searchQuery.trim().toLowerCase();
    if (q) {
      out = out.filter(
        (c) =>
          (c.sourceChannel ?? "").toLowerCase().includes(q) ||
          (c.email ?? "").toLowerCase().includes(q),
      );
    }
    return out;
  })();

  // switchGroup / Tierfilter / reset on search toNo. 1 page,avoid emptypage
  useEffect(() => {
    setCurrentPage(1);
  }, [groupFilter, tierFilter, searchQuery]);

  // pageSize === 0 represents“All”:singlepageaccommodateAllalready filteredCredential
  const effectivePageSize =
    pageSize === 0 ? Math.max(filteredCredentials.length, 1) : pageSize;
  const totalPages = Math.max(
    1,
    Math.ceil(filteredCredentials.length / effectivePageSize),
  );
  const startIndex = (currentPage - 1) * effectivePageSize;
  const endIndex = startIndex + effectivePageSize;
  const serverPageCreds = filteredCredentials.slice(startIndex, endIndex);
  // drag to reorderoflocal optimistic order:only when id the set and the currentpagetakes effect when consistent,otherwise fall back to server order,
  // avoid pagingpage / order becomes scrambled after data change.
  const [pageOrder, setPageOrder] = useState<number[] | null>(null);
  const currentCredentials = (() => {
    if (!pageOrder) return serverPageCreds;
    const serverIds = new Set(serverPageCreds.map((c) => c.id));
    const orderIds = new Set(pageOrder);
    if (
      serverIds.size !== orderIds.size ||
      ![...serverIds].every((id) => orderIds.has(id))
    ) {
      return serverPageCreds;
    }
    const byId = new Map(serverPageCreds.map((c) => [c.id, c]));
    return pageOrder.map((id) => byId.get(id)!).filter(Boolean);
  })();
  const currentPageIds = currentCredentials.map((c) => c.id);
  const currentPageAllSelected =
    currentPageIds.length > 0 &&
    currentPageIds.every((id) => selectedIds.has(id));
  const allFilteredIds = filteredCredentials.map((c) => c.id);
  const allFilteredSelected =
    allFilteredIds.length > 0 &&
    allFilteredIds.every((id) => selectedIds.has(id));

  // pagepageclear the local sort override at that time,return to server order
  useEffect(() => {
    setPageOrder(null);
  }, [currentPage]);

  const dragSensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
  );

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const ids = currentCredentials.map((c) => c.id);
    const oldIndex = ids.indexOf(Number(active.id));
    const newIndex = ids.indexOf(Number(over.id));
    if (oldIndex < 0 || newIndex < 0) return;

    const newOrder = arrayMove(ids, oldIndex, newIndex);
    setPageOrder(newOrder);

    // assign continuous increasing values in new visual orderof priority(Globalposition = startIndex + pageinner index).
    // does not depend on the originalhas priority value range:even if the original value is allasDefault 0 / identical,still ensures numbers update,persist the sort order;
    // crosspagedoes not conflict either(No. 1 page 0..11,No. 2 page 12..23).only for actual changesofCardsendRequest.
    const prevPriority = new Map(
      currentCredentials.map((c) => [c.id, c.priority]),
    );
    const updates = newOrder
      .map((id, i) => ({ id, priority: startIndex + i }))
      .filter((u) => prevPriority.get(u.id) !== u.priority);
    if (updates.length === 0) return;

    Promise.all(
      updates.map((u) =>
        setPriority.mutateAsync({ id: u.id, priority: u.priority }),
      ),
    )
      .then(() => {
        toast.success("Priority order updated");
        queryClient.invalidateQueries({ queryKey: ["credentials"] });
      })
      .catch((err) => {
        toast.error("Update priority failed: " + (err as Error).message);
        setPageOrder(null);
      });
  };

  const gridRef = useRef<HTMLElement | null>(null);
  const rectSelection = useRectSelect({
    containerRef: gridRef,
    itemSelector: "[data-credential-id]",
    idAttribute: "credential-id",
    enabled: currentCredentials.length > 0,
    onSelectionChange: (hits, additive) => {
      setSelectedIds((prev) => {
        if (!additive) return new Set(hits);
        const next = new Set(prev);
        hits.forEach((id) => next.add(id));
        return next;
      });
    },
  });
  const disabledCredentialCount =
    data?.credentials.filter((c) => c.disabled).length || 0;

  // Over quotaand not yetDisableofCount(Used forone clickOveragebutton)
  const quotaExceededCount = (data?.credentials || []).filter((c) => {
    if (c.disabled) return false;
    const b = balanceMap.get(c.id) || c.balance;
    if (!b) return false;
    return b.remaining <= 0 || b.usagePercentage >= 100;
  }).length;

  // Overagestatistics:compute separately"Enabled / Not enabled / Pending"three kinds,helps button text and decisions
  const overageStats = (() => {
    let enabled = 0;
    let disabledOff = 0;
    let unknown = 0;
    let total = 0;
    for (const c of data?.credentials || []) {
      if (c.disabled) continue;
      total += 1;
      const b = balanceMap.get(c.id) || c.balance;
      if (!b) {
        // not fetched yetBalance,cannot decide — viewaspending
        unknown += 1;
        continue;
      }
      // cannotEnableofsubscription(FREE)not counted in statistics
      if (b.overageCapable === false) continue;
      if (b.overageEnabled === true) enabled += 1;
      else if (b.overageCapable === true) disabledOff += 1;
      else unknown += 1;
    }
    return { enabled, disabledOff, unknown, total };
  })();
  const overageEnableableCount = overageStats.disabledOff;
  const overageRetryableCount = overageStats.disabledOff + overageStats.unknown;

  useEffect(() => {
    setCurrentPage(1);
  }, [data?.credentials.length]);

  useEffect(() => {
    if (!data?.credentials) {
      setBalanceMap(new Map());
      setLoadingBalanceIds(new Set());
      return;
    }
    const validIds = new Set(data.credentials.map((c) => c.id));
    setBalanceMap((prev) => {
      const next = new Map<number, BalanceResponse>();
      prev.forEach((v, id) => {
        if (validIds.has(id)) next.set(id, v);
      });
      return next.size === prev.size ? prev : next;
    });
    setLoadingBalanceIds((prev) => {
      if (prev.size === 0) return prev;
      const next = new Set<number>();
      prev.forEach((id) => {
        if (validIds.has(id)) next.add(id);
      });
      return next.size === prev.size ? prev : next;
    });
  }, [data?.credentials]);

  const toggleDarkMode = () => {
    setDarkMode(!darkMode);
    document.documentElement.classList.toggle("dark");
  };

  const handleRefresh = () => {
    refetch();
    toast.success("Credential list refreshed");
  };

  const handleLogout = () => {
    storage.removeApiKey();
    queryClient.clear();
    onLogout();
  };

  useEffect(() => {
    if (!error) return;
    const parsed = parseError(error);
    if (parsed.type === "authentication_error") {
      toast.error("Login expired. Please log in again");
      handleLogout();
    }
  }, [error]);

  const toggleSelect = (id: number) => {
    const next = new Set(selectedIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelectedIds(next);
  };
  const deselectAll = () => setSelectedIds(new Set());

  /** select all / CancelSelect the current pageCredential.Selectedothers in the setpageofwill not beClear. */
  const toggleSelectCurrentPage = () => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (currentPageAllSelected) {
        currentPageIds.forEach((id) => next.delete(id));
      } else {
        currentPageIds.forEach((id) => next.add(id));
      }
      return next;
    });
  };

  /** select all / Deselect allplacehasafter filteringofCredential(crosspage) */
  const toggleSelectAllFiltered = () => {
    if (allFilteredSelected) {
      // Cancel:onlyClear filterwithin rangeof,keep items outside the filter rangeofSelecteditem
      setSelectedIds((prev) => {
        const next = new Set(prev);
        allFilteredIds.forEach((id) => next.delete(id));
        return next;
      });
    } else {
      setSelectedIds(new Set(allFilteredIds));
    }
  };

  const handleBatchDelete = async () => {
    if (selectedIds.size === 0) {
      toast.error("Please select the credentials to delete first");
      return;
    }
    const ids = Array.from(selectedIds);
    if (
      !(await confirm({
        title: "Bulk delete credentials",
        description: `Are you sure you want to delete ${ids.length} credentials? This action cannot be undone.`,
        confirmText: "Delete",
        destructive: true,
      }))
    )
      return;
    let s = 0,
      f = 0;
    for (const id of ids) {
      try {
        await new Promise<void>((resolve, reject) => {
          deleteCredential(id, {
            onSuccess: () => {
              s++;
              resolve();
            },
            onError: (err) => {
              f++;
              reject(err);
            },
          });
        });
      } catch {}
    }
    if (f === 0) toast.success(`Deleted successfully ${s} credentials`);
    else toast.warning(`Delete credential: success ${s} succeeded, failed ${f} items`);
    deselectAll();
  };

  const handleBatchResetFailure = async () => {
    if (selectedIds.size === 0) {
      toast.error("Please select the credentials to restore first");
      return;
    }
    const failedIds = Array.from(selectedIds).filter((id) => {
      const c = data?.credentials.find((x) => x.id === id);
      return c && c.failureCount > 0;
    });
    if (failedIds.length === 0) {
      toast.error("None of the selected credentials have failed");
      return;
    }
    let s = 0,
      f = 0;
    for (const id of failedIds) {
      try {
        await new Promise<void>((resolve, reject) => {
          resetFailure(id, {
            onSuccess: () => {
              s++;
              resolve();
            },
            onError: (err) => {
              f++;
              reject(err);
            },
          });
        });
      } catch {}
    }
    if (f === 0) toast.success(`Restored successfully ${s} credentials`);
    else toast.warning(`Success ${s} succeeded, failed ${f} items`);
    deselectAll();
  };

  const handleBatchForceRefresh = async () => {
    if (selectedIds.size === 0) {
      toast.error("Please select the credentials to refresh first");
      return;
    }
    const enabledIds = Array.from(selectedIds).filter((id) => {
      const c = data?.credentials.find((x) => x.id === id);
      return c && !c.disabled;
    });
    if (enabledIds.length === 0) {
      toast.error("None of the selected credentials are enabled");
      return;
    }
    setBatchRefreshing(true);
    setBatchRefreshProgress({ current: 0, total: enabledIds.length });
    let s = 0,
      f = 0;
    for (let i = 0; i < enabledIds.length; i++) {
      try {
        await forceRefreshToken(enabledIds[i]);
        s++;
      } catch {
        f++;
      }
      setBatchRefreshProgress({ current: i + 1, total: enabledIds.length });
    }
    setBatchRefreshing(false);
    queryClient.invalidateQueries({ queryKey: ["credentials"] });
    if (f === 0) toast.success(`Refreshed successfully ${s} credentials Token`);
    else toast.warning(`Refresh Token: success ${s} succeeded, failed ${f} items`);
    deselectAll();
  };

  const handleClearAll = async () => {
    if (!data?.credentials || data.credentials.length === 0) {
      toast.error("No credentials to clear");
      return;
    }
    const disabled = data.credentials.filter((c) => c.disabled);
    if (disabled.length === 0) {
      toast.error("No disabled credentials to clear");
      return;
    }
    if (
      !(await confirm({
        title: "Clear disabled credentials",
        description: `Are you sure you want to clear all ${disabled.length} disabled credentials? This action cannot be undone.`,
        confirmText: "Clear",
        destructive: true,
      }))
    )
      return;
    let s = 0,
      f = 0;
    for (const c of disabled) {
      try {
        await new Promise<void>((resolve, reject) => {
          deleteCredential(c.id, {
            onSuccess: () => {
              s++;
              resolve();
            },
            onError: (err) => {
              f++;
              reject(err);
            },
          });
        });
      } catch {}
    }
    if (f === 0) toast.success(`Successfully cleared all ${s} disabled credentials`);
    else toast.warning(`Clear disabled credentials: success ${s} succeeded, failed ${f} items`);
    deselectAll();
  };

  const handleQueryCurrentPageInfo = async () => {
    if (currentCredentials.length === 0) {
      toast.error("No credentials on the current page can be queried");
      return;
    }
    const ids = currentCredentials.filter((c) => !c.disabled).map((c) => c.id);
    if (ids.length === 0) {
      toast.error("No enabled credentials on the current page can be queried");
      return;
    }
    setQueryingInfo(true);
    setQueryInfoProgress({ current: 0, total: ids.length });
    // hasconcurrent(worker pool,andBulk validateconsistent),one by oneitemsupdateBalanceand progress
    let s = 0;
    let f = 0;
    let finalized = 0;
    let next = 0;
    const CONCURRENCY = 8;
    const worker = async () => {
      while (true) {
        const i = next++;
        if (i >= ids.length) return;
        const id = ids[i];
        setLoadingBalanceIds((prev) => {
          const n = new Set(prev);
          n.add(id);
          return n;
        });
        try {
          const balance = await getCredentialBalance(id);
          s++;
          setBalanceMap((prev) => {
            const n = new Map(prev);
            n.set(id, balance);
            return n;
          });
        } catch {
          f++;
        } finally {
          setLoadingBalanceIds((prev) => {
            const n = new Set(prev);
            n.delete(id);
            return n;
          });
        }
        finalized++;
        setQueryInfoProgress({ current: finalized, total: ids.length });
      }
    };
    await Promise.all(
      Array.from({ length: Math.min(CONCURRENCY, ids.length) }, () => worker()),
    );
    setQueryingInfo(false);
    if (f === 0) toast.success(`Query complete: success ${s}/${ids.length}`);
    else toast.warning(`Query complete: success ${s} succeeded, failed ${f} items`);
  };

  const handleRefreshBalance = async (id: number) => {
    setLoadingBalanceIds((prev) => {
      const n = new Set(prev);
      n.add(id);
      return n;
    });
    try {
      const balance = await getCredentialBalance(id);
      setBalanceMap((prev) => {
        const n = new Map(prev);
        n.set(id, balance);
        return n;
      });
      toast.success("Balance refreshed");
    } catch (err) {
      toast.error("Refresh balance failed: " + (err as Error).message);
    } finally {
      setLoadingBalanceIds((prev) => {
        const n = new Set(prev);
        n.delete(id);
        return n;
      });
    }
  };

  const handleBatchVerify = async () => {
    if (selectedIds.size === 0) {
      toast.error("Please select the credentials to validate first");
      return;
    }
    setVerifying(true);
    cancelVerifyRef.current = false;
    const ids = Array.from(selectedIds);
    setVerifyProgress({ current: 0, total: ids.length });

    // id → email,helps the resultListdirectly see which oneaccounts
    const emailById = new Map<number, string | undefined>();
    for (const c of data?.credentials ?? []) emailById.set(c.id, c.email);

    const init = new Map<number, VerifyResult>();
    ids.forEach((id) =>
      init.set(id, { id, status: "pending", email: emailById.get(id) }),
    );
    setVerifyResults(init);
    setVerifyDialogOpen(true);

    // hasconcurrent(none 2s interval).worker pool claim the nextitems id,one by oneitemsupdate the result.
    let successCount = 0;
    let finalized = 0;
    let next = 0;
    const CONCURRENCY = 8;
    const worker = async () => {
      while (true) {
        if (cancelVerifyRef.current) return;
        const i = next++;
        if (i >= ids.length) return;
        const id = ids[i];
        setVerifyResults((prev) => {
          const n = new Map(prev);
          n.set(id, { id, status: "verifying", email: emailById.get(id) });
          return n;
        });
        try {
          const balance = await getCredentialBalance(id);
          successCount++;
          setVerifyResults((prev) => {
            const n = new Map(prev);
            n.set(id, {
              id,
              status: "success",
              usage: `${balance.currentUsage}/${balance.usageLimit}`,
              email: emailById.get(id),
            });
            return n;
          });
        } catch (err) {
          setVerifyResults((prev) => {
            const n = new Map(prev);
            n.set(id, {
              id,
              status: "failed",
              error: extractErrorMessage(err),
              email: emailById.get(id),
            });
            return n;
          });
        }
        finalized++;
        setVerifyProgress({ current: finalized, total: ids.length });
      }
    };
    await Promise.all(
      Array.from({ length: Math.min(CONCURRENCY, ids.length) }, () => worker()),
    );
    setVerifying(false);
    if (!cancelVerifyRef.current)
      toast.success(`Validation complete: success ${successCount}/${ids.length}`);
  };

  const handleCancelVerify = () => {
    cancelVerifyRef.current = true;
    setVerifying(false);
  };

  // inBulk validatewindowDeletesinglefailed credentials
  const handleDeleteVerifyResult = (id: number) => {
    deleteCredential(id, {
      onSuccess: () => {
        setVerifyResults((prev) => {
          const n = new Map(prev);
          n.delete(id);
          return n;
        });
        toast.success(`Credential #${id} Deleted`);
      },
      onError: (err) => toast.error("Delete failed: " + extractErrorMessage(err)),
    });
  };

  // one clickDeleteBulk validateinside the windowAllFailedCredential(concurrentDelete)
  const handleDeleteFailedVerify = () => {
    const failedIds = Array.from(verifyResults.values())
      .filter((r) => r.status === "failed")
      .map((r) => r.id);
    if (failedIds.length === 0) return;
    setVerifyDeleting(true);
    let remaining = failedIds.length;
    let ok = 0;
    failedIds.forEach((id) => {
      deleteCredential(id, {
        onSuccess: () => {
          ok++;
          setVerifyResults((prev) => {
            const n = new Map(prev);
            n.delete(id);
            return n;
          });
        },
        onError: (err) =>
          toast.error(`Delete #${id} Failed: ` + extractErrorMessage(err)),
        onSettled: () => {
          remaining--;
          if (remaining === 0) {
            setVerifyDeleting(false);
            toast.success(`Deleted ${ok}/${failedIds.length} failed credentials`);
          }
        },
      });
    });
  };

  // one clickOverage:putplacehasOver quota(notDisable)Credentialmarkas QuotaExceeded andDisable
  const [disablingQuota, setDisablingQuota] = useState(false);
  const handleDisableQuotaExceeded = async () => {
    if (quotaExceededCount === 0) {
      toast.info('There are no over-quota credentials right now. You can first click"Refresh the balance on the current page"');
      return;
    }
    if (
      !(await confirm({
        title: "Disable over-quota credentials",
        description: `Are you sure you want to put ${quotaExceededCount} over-quota credentials? Disable all of them?`,
        confirmText: "Disable",
        destructive: true,
      }))
    )
      return;
    setDisablingQuota(true);
    try {
      const res = await disableQuotaExceeded();
      const ok = res.disabledIds?.length || 0;
      const skip = res.skippedIds?.length || 0;
      if (ok > 0)
        toast.success(
          `Disabled ${ok} over-quota credentials${skip > 0 ? `, skipped ${skip} items` : ""}`,
        );
      else toast.warning("No over-quota credentials found (the cache may be stale)");
      queryClient.invalidateQueries({ queryKey: ["credentials"] });
    } catch (err) {
      toast.error("One-click overage failed: " + extractErrorMessage(err));
    } finally {
      setDisablingQuota(false);
    }
  };

  // one clickEnable overage:CallsUpstream setUserPreference putplacehas"canEnableandNot enabledstart"ofCredentialEnable
  const [enablingOverage, setEnablingOverage] = useState(false);
  const handleEnableOverageAll = async () => {
    if (overageEnableableCount === 0) {
      toast.info("There are currently no credentials clearly marked as overage not enabled");
      return;
    }
    if (
      !(await confirm({
        title: "Enable overage",
        description: `Are you sure you want to, for ${overageEnableableCount} credentials? Once enabled, usage beyond the quota is billed as overageRate billing.`,
        confirmText: "Enable",
      }))
    )
      return;
    setEnablingOverage(true);
    try {
      const res = await enableOverageForAllCapable();
      const ok = res.enabledIds?.length || 0;
      const fail = res.failedIds?.length || 0;
      if (ok > 0 && fail === 0) toast.success(`Done for ${ok} credentials with overage enabled`);
      else if (ok > 0 && fail > 0)
        toast.warning(
          `Success ${ok} succeeded, failed ${fail} items:${overageFailureMessage(res.failureMessages?.[0])}`,
        );
      else if (fail > 0)
        toast.error(
          `All failed:${overageFailureMessage(res.failureMessages?.[0])}`,
        );
      else toast.info("No credentials to act on");
      queryClient.invalidateQueries({ queryKey: ["credentials"] });
    } catch (err) {
      toast.error("One-click enable overage failed: " + extractErrorMessage(err));
    } finally {
      setEnablingOverage(false);
    }
  };

  // RetryfetchOverageStatus:only forStatusPendingofCredentialBulkqueryBalance(read only,safe).
  // distinct from [one clickEnable overage]——the latter willCallswrite endpoint setUserPreference,FREE the subscription will 403.
  const [refreshingOverage, setRefreshingOverage] = useState(false);
  const [refreshingOverageProgress, setRefreshingOverageProgress] = useState({
    current: 0,
    total: 0,
  });
  const handleRefreshOverageStatus = async () => {
    const targets = (data?.credentials || [])
      .filter((c) => {
        if (c.disabled) return false;
        const b = balanceMap.get(c.id) || c.balance;
        if (!b) return true;
        return b.overageCapable === undefined || b.overageCapable === null;
      })
      .map((c) => c.id);
    if (targets.length === 0) {
      toast.info("No credentials with pending status");
      return;
    }
    setRefreshingOverage(true);
    setRefreshingOverageProgress({ current: 0, total: targets.length });
    let s = 0,
      f = 0;
    for (let i = 0; i < targets.length; i++) {
      const id = targets[i];
      setLoadingBalanceIds((prev) => {
        const n = new Set(prev);
        n.add(id);
        return n;
      });
      try {
        const balance = await getCredentialBalance(id);
        s++;
        setBalanceMap((prev) => {
          const n = new Map(prev);
          n.set(id, balance);
          return n;
        });
      } catch {
        f++;
      } finally {
        setLoadingBalanceIds((prev) => {
          const n = new Set(prev);
          n.delete(id);
          return n;
        });
      }
      setRefreshingOverageProgress({ current: i + 1, total: targets.length });
    }
    setRefreshingOverage(false);
    if (f === 0) toast.success(`Refresh complete: success ${s}/${targets.length}`);
    else toast.warning(`Refresh complete: success ${s} succeeded, failed ${f} items`);
  };

  const [exportingKam, setExportingKam] = useState(false);

  const handleUpdateAdminKey = async (e: React.FormEvent) => {
    e.preventDefault();
    const key = newAdminKey.trim();
    if (!key) {
      toast.error("New loginAPIKey cannot be empty");
      return;
    }
    setUpdatingAdminKey(true);
    try {
      await updateAdminKey({ newKey: key });
      storage.setApiKey(key);
      toast.success("LoginAPIThe key has been updated and automatically switched to the new one Key");
      setAdminKeyDialogOpen(false);
      setNewAdminKey("");
    } catch (error) {
      toast.error(`Update failed: ${extractErrorMessage(error)}`);
    } finally {
      setUpdatingAdminKey(false);
    }
  };

  const handleExportKam = async () => {
    if (selectedIds.size === 0) {
      toast.info("Please check the credentials to export first");
      return;
    }
    const ids = Array.from(selectedIds);
    setExportingKam(true);
    try {
      const exportData = await exportKamCredentials(ids);
      const accountCount = exportData.accounts?.length ?? 0;
      if (accountCount === 0) {
        toast.warning("None of the selected credentials can be exported (missing refreshToken)");
        return;
      }
      const json = JSON.stringify(exportData, null, 2);
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const ts = new Date()
        .toISOString()
        .replace(/[:.]/g, "-")
        .replace("T", "_")
        .slice(0, 19);
      const a = document.createElement("a");
      a.href = url;
      a.download = `kiro-account-manager-export-${ts}.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      const skipped = ids.length - accountCount;
      toast.success(
        skipped > 0
          ? `Exported ${accountCount} accounts,${skipped} invalid ones skipped`
          : `Exported ${accountCount} accounts`,
      );
    } catch (err) {
      toast.error("Export failed: " + extractErrorMessage(err));
    } finally {
      setExportingKam(false);
    }
  };

  const handleToggleLoadBalancing = () => {
    const cur = loadBalancingData?.mode || "priority";
    const next = cur === "priority" ? "balanced" : "priority";
    setLoadBalancingMode(next, {
      onSuccess: () =>
        toast.success(
          `Switched to${next === "priority" ? "Priority mode" : "Balanced load mode"}`,
        ),
      onError: (err) => toast.error(`Switch failed: ${extractErrorMessage(err)}`),
    });
  };

  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <div className="text-center">
          <div className="animate-spin rounded-full h-10 w-10 border-2 border-primary/20 border-t-primary mx-auto mb-4"></div>
          <p className="text-sm text-muted-foreground">Loading…</p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="min-h-screen flex items-center justify-center p-4">
        <Card className="w-full max-w-md">
          <CardContent className="pt-6 text-center">
            <div className="text-destructive font-semibold mb-2">Load failed</div>
            <p className="text-sm text-muted-foreground mb-4">
              {extractErrorMessage(error)}
            </p>
            <div className="flex gap-2 justify-center">
              <Button onClick={() => refetch()}>Retry</Button>
              <Button variant="outline" onClick={handleLogout}>
                Log in again
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className={embedded ? "" : "min-h-screen"}>
      {/* frosted glass navigation at the topitems(render only in standalone mode;embedded modebyouter layer App provide the top bar) */}
      {!embedded && (
        <header className="sticky top-0 z-40 w-full glass">
          <div className="mx-auto max-w-[1400px] flex h-16 items-center justify-between px-4 md:px-8">
            <div className="flex items-center gap-2.5">
              <img
                src="/admin/kirors.png"
                alt="Kiro"
                className="h-10 w-10 object-contain"
                draggable={false}
              />
              <span className="font-semibold tracking-tight">Kiro Admin</span>
            </div>
            <div className="flex items-center gap-1.5">
              <Button
                variant="outline"
                size="sm"
                onClick={handleToggleLoadBalancing}
                disabled={isLoadingMode || isSettingMode}
                title="Switch load balancing mode"
              >
                <Activity className="h-3.5 w-3.5" />
                {isLoadingMode
                  ? "Loading…"
                  : loadBalancingData?.mode === "priority"
                    ? "Priority"
                    : "Balanced load"}
              </Button>
              <Button variant="ghost" size="icon" asChild title="GitHub Repository">
                <a
                  href="https://github.com/ZyphrZero/kiro.rs"
                  target="_blank"
                  rel="noopener noreferrer"
                  aria-label="GitHub Repository"
                >
                  <GithubIcon className="h-4 w-4" />
                </a>
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={toggleDarkMode}
                title="Switch theme"
              >
                {darkMode ? (
                  <Sun className="h-4 w-4" />
                ) : (
                  <Moon className="h-4 w-4" />
                )}
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={handleRefresh}
                title="Refresh"
              >
                <RefreshCw className="h-4 w-4" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => setImageUpdateDialogOpen(true)}
                title={
                  updateCheck?.hasUpdate
                    ? `New version found v${updateCheck.latestVersion}(currently v${updateCheck.currentVersion})`
                    : "Mirror online update"
                }
                className="relative"
              >
                <UploadCloud className="h-4 w-4" />
                {updateCheck?.hasUpdate && (
                  <span className="absolute right-1 top-1 inline-flex h-2 w-2 items-center justify-center">
                    <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-red-400 opacity-75" />
                    <span className="relative inline-flex h-2 w-2 rounded-full bg-red-500" />
                  </span>
                )}
              </Button>
              <DropdownMenu modal={false}>
                <DropdownMenuTrigger asChild>
                  <Button variant="ghost" size="icon" title="Settings">
                    <Settings className="h-4 w-4" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuLabel>Key management</DropdownMenuLabel>
                  <DropdownMenuItem
                    onSelect={() => {
                      setNewAdminKey("");
                      setShowAdminKeyPlain(false);
                      setAdminKeyDialogOpen(true);
                    }}
                  >
                    <Key />
                    Change loginAPIKey (admin panel login)
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
              <Button
                variant="ghost"
                size="icon"
                onClick={handleLogout}
                title="Log out"
              >
                <LogOut className="h-4 w-4" />
              </Button>
            </div>
          </div>
        </header>
      )}

      {/* main content */}
      <main
        ref={gridRef}
        className={embedded ? "" : "mx-auto max-w-[1400px] px-4 md:px-8 py-8"}
      >
        {/* large title */}
        <div className="mb-5 flex items-end justify-between gap-4 sm:mb-6">
          <div>
            <h1 className="text-2xl font-semibold tracking-tight leading-tight sm:text-[28px]">
              Credential management
            </h1>
            <p className="mt-1 text-sm text-muted-foreground">
              Manage Kiro all access credentials, load balancing, and login info of
            </p>
          </div>
        </div>

        {/* statisticsCard */}
        <div className="mb-5 grid grid-cols-3 gap-2 sm:mb-6 sm:gap-4">
          <Card className="hover:shadow-apple-lg hover:-translate-y-0.5">
            <CardContent className="p-3 sm:p-5">
              <div className="text-[11px] font-medium text-muted-foreground sm:text-[13px]">
                Total credentials
              </div>
              <div className="mt-1.5 text-2xl font-semibold tracking-tight tabular-nums sm:mt-2 sm:text-3xl">
                {formatNumber(data?.total)}
              </div>
            </CardContent>
          </Card>
          <Card className="hover:shadow-apple-lg hover:-translate-y-0.5">
            <CardContent className="p-3 sm:p-5">
              <div className="text-[11px] font-medium text-muted-foreground sm:text-[13px]">
                Available credentials
              </div>
              <div className="mt-1.5 text-2xl font-semibold tracking-tight tabular-nums text-emerald-600 dark:text-emerald-400 sm:mt-2 sm:text-3xl">
                {formatNumber(data?.available)}
              </div>
            </CardContent>
          </Card>
          <Card className="hover:shadow-apple-lg hover:-translate-y-0.5">
            <CardContent className="p-3 sm:p-5">
              <div className="text-[11px] font-medium text-muted-foreground sm:text-[13px]">
                Currently active
              </div>
              <div className="mt-1.5 flex min-w-0 flex-wrap items-center gap-1.5 sm:mt-2 sm:gap-2">
                <span className="truncate text-2xl font-semibold tracking-tight tabular-nums sm:text-3xl">
                  #{data?.currentId || "-"}
                </span>
                {data?.currentId && <Badge variant="success">Active</Badge>}
              </div>
            </CardContent>
          </Card>
        </div>

        {/* toolbar */}
        <div className="mb-5 flex flex-col gap-3">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            <h2 className="text-lg font-semibold tracking-tight">Credential list</h2>
            {data?.credentials && data.credentials.length > 0 && (
              <Badge variant="secondary">
                {groupFilter || tierFilter.size > 0
                  ? `${filteredCredentials.length} / ${data.credentials.length}`
                  : data.credentials.length}
              </Badge>
            )}
            {groupFilter && (
              <Badge variant="outline" className="gap-1">
                Filter:{groupFilter === "__none__" ? "Ungrouped" : groupFilter}
                <button
                  type="button"
                  className="ml-1 text-muted-foreground hover:text-foreground"
                  onClick={() => setGroupFilter("")}
                  title="Clear filter"
                >
                  ×
                </button>
              </Badge>
            )}
            {tierFilter.size > 0 && (
              <Badge variant="outline" className="gap-1">
                Tier:
                {Array.from(tierFilter)
                  .map((t) => TIER_LABELS[t])
                  .join(",")}
                <button
                  type="button"
                  className="ml-1 text-muted-foreground hover:text-foreground"
                  onClick={() => setTierFilter(new Set())}
                  title="Clear tier filter"
                >
                  ×
                </button>
              </Badge>
            )}

            {currentCredentials.length > 0 && (
              <Button
                size="sm"
                variant="ghost"
                className="px-2 sm:px-3"
                onClick={toggleSelectCurrentPage}
                title={currentPageAllSelected ? "Deselect the current page" : "Select the current page"}
              >
                {currentPageAllSelected ? "Deselect all" : "Select the current page"}
              </Button>
            )}
            {filteredCredentials.length > currentCredentials.length && (
              <Button
                size="sm"
                variant="ghost"
                className="px-2 sm:px-3"
                onClick={toggleSelectAllFiltered}
                title={
                  allFilteredSelected
                    ? "Deselect all filtered results"
                    : `Select all ${filteredCredentials.length} filtered results`
                }
              >
                {allFilteredSelected
                  ? "Deselect all pages"
                  : `Select all pages (${filteredCredentials.length})`}
              </Button>
            )}
            {selectedIds.size > 0 && (
              <>
                <Badge variant="default">Selected {selectedIds.size}</Badge>
                <Button
                  onClick={deselectAll}
                  size="sm"
                  variant="ghost"
                  className="px-2 sm:px-3"
                >
                  Deselect
                </Button>
              </>
            )}
            {verifying && !verifyDialogOpen && (
              <Button
                onClick={() => setVerifyDialogOpen(true)}
                size="sm"
                variant="secondary"
              >
                <CheckCircle2 className="h-3.5 w-3.5 animate-spin" />
                Validating… {verifyProgress.current}/{verifyProgress.total}
              </Button>
            )}
          </div>

          {/* No.second row:filter(left) + Actions(right) */}
          <div className="flex w-full flex-col gap-2 sm:flex-row sm:flex-wrap sm:items-center">
            {/* filter — left(two column grid side by side on mobile,inline on desktop) */}
            <div className="grid w-full grid-cols-2 gap-2 sm:flex sm:w-auto sm:flex-wrap sm:items-center">
              {/* fuzzy search:Source channel(Note)/ Email;full row on mobile,desktop 200px */}
              <div className="relative col-span-2 sm:col-span-1 sm:w-[200px]">
                <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder="Search source channel / Note / Email"
                  className="h-8 w-full rounded-full border border-border bg-card/60 pl-5 pr-5 text-base backdrop-blur placeholder:text-muted-foreground/70 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring sm:text-sm"
                />
                {searchQuery && (
                  <button
                    type="button"
                    onClick={() => setSearchQuery("")}
                    className="absolute right-2 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-full text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                    title="Clear search"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                )}
              </div>
              <Select
                value={groupFilter || "all"}
                onValueChange={(v) => setGroupFilter(v === "all" ? "" : v)}
              >
                <SelectTrigger
                  className="h-8 w-full rounded-full border-border bg-card/60 px-3 backdrop-blur sm:w-[140px]"
                  title="Filter credentials by group"
                >
                  <SelectValue placeholder="All groups" />
                </SelectTrigger>
                <SelectContent align="end">
                  <SelectItem value="all">All groups</SelectItem>
                  <SelectItem value="__none__">Ungrouped</SelectItem>
                  {groupOptions.map((g) => (
                    <SelectItem key={g} value={g}>
                      {g}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>

              {/* Subscription tierfilter(multi select) */}
              <DropdownMenu modal={false}>
                <DropdownMenuTrigger asChild>
                  <button
                    type="button"
                    title="Filter credentials by subscription tier (multi-select, based on the latest balance cache)"
                    className="inline-flex h-8 w-full items-center justify-between gap-1 rounded-full border border-border bg-card/60 px-3 text-sm backdrop-blur hover:bg-accent sm:w-[136px]"
                  >
                    <span className="truncate">
                      {tierFilter.size > 0
                        ? `Tier ·${tierFilter.size}`
                        : "All tiers"}
                    </span>
                    <ChevronDown className="h-3.5 w-3.5 opacity-60" />
                  </button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="min-w-[10rem]">
                  <DropdownMenuLabel>Subscription tier</DropdownMenuLabel>
                  {TIER_OPTIONS.map((t) => (
                    <DropdownMenuItem
                      key={t.value}
                      onSelect={(e) => {
                        e.preventDefault();
                        toggleTier(t.value);
                      }}
                      className="gap-2"
                    >
                      <Checkbox checked={tierFilter.has(t.value)} />
                      <span>{t.label}</span>
                    </DropdownMenuItem>
                  ))}
                  {tierFilter.size > 0 && (
                    <>
                      <DropdownMenuSeparator />
                      <DropdownMenuItem
                        onSelect={(e) => {
                          e.preventDefault();
                          setTierFilter(new Set());
                        }}
                        className="text-muted-foreground"
                      >
                        Clear tier filter
                      </DropdownMenuItem>
                    </>
                  )}
                </DropdownMenuContent>
              </DropdownMenu>

              {/* Card / List view switch(iOS segmented control) */}
              <div className="col-span-2 inline-flex h-8 shrink-0 items-center justify-self-start rounded-full border border-border bg-card/60 p-0.5 backdrop-blur sm:col-span-1">
                <button
                  type="button"
                  onClick={() => changeViewMode("card")}
                  aria-pressed={viewMode === "card"}
                  title="Card view"
                  className={`inline-flex h-7 items-center gap-1 rounded-full px-2.5 text-[13px] transition-colors ${
                    viewMode === "card"
                      ? "bg-background text-foreground shadow-apple-sm"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <LayoutGrid className="h-3.5 w-3.5" />
                  <span className="hidden sm:inline">Card</span>
                </button>
                <button
                  type="button"
                  onClick={() => changeViewMode("list")}
                  aria-pressed={viewMode === "list"}
                  title="List view"
                  className={`inline-flex h-7 items-center gap-1 rounded-full px-2.5 text-[13px] transition-colors ${
                    viewMode === "list"
                      ? "bg-background text-foreground shadow-apple-sm"
                      : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  <List className="h-3.5 w-3.5" />
                  <span className="hidden sm:inline">List</span>
                </button>
              </div>
            </div>

            {/* Actions — right(full width two column grid on mobile,right aligned inline on desktop) */}
            <div className="ml-auto grid w-full grid-cols-2 gap-2 sm:flex sm:w-auto sm:flex-wrap sm:items-center">
              {selectedIds.size > 0 && (
                <>
                  <Button
                    onClick={() => setBatchEditDialogOpen(true)}
                    size="sm"
                    variant="outline"
                    title="Bulk edit groups / Source channel"
                  >
                    <Tags className="h-3.5 w-3.5" />
                    Group/Source
                  </Button>
                  <Button
                    onClick={handleBatchDelete}
                    size="sm"
                    variant="destructive"
                    className="w-full sm:w-auto"
                    disabled={selectedIds.size === 0}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                    Delete
                  </Button>
                  <span className="mx-1 hidden h-5 w-px bg-border/70 sm:inline-block" />
                </>
              )}

              {/* primaryActions */}
              <Button
                onClick={() => setAddDialogOpen(true)}
                size="sm"
                className="w-full sm:w-auto"
              >
                <Plus className="h-3.5 w-3.5" />
                Add credential
              </Button>

              {/* Import / Logincollapsible menu */}
              <DropdownMenu modal={false}>
                <DropdownMenuTrigger asChild>
                  <Button
                    size="sm"
                    variant="outline"
                    className="w-full sm:w-auto"
                  >
                    <Upload className="h-3.5 w-3.5" />
                    Login / Import / Export
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuLabel>Login</DropdownMenuLabel>
                  <DropdownMenuItem
                    onSelect={() => setSocialLoginDialogOpen(true)}
                  >
                    <LogIn />
                    Kiro Account login
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onSelect={() => setIdcLoginDialogOpen(true)}
                  >
                    <Key />
                    AWS SSO (IdC) Login
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onSelect={() => setEnterpriseLoginDialogOpen(true)}
                  >
                    <Building2 />
                    Enterprise (IAM Identity Center) Login
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuLabel>Import</DropdownMenuLabel>
                  <DropdownMenuItem
                    onSelect={() => setBatchImportDialogOpen(true)}
                  >
                    <Upload />
                    Bulk import
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onSelect={() => setKamImportDialogOpen(true)}
                  >
                    <FileUp />
                    Kiro Account Manager Import
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onSelect={handleExportKam}
                    disabled={exportingKam}
                  >
                    <FileDown />
                    {exportingKam ? "Exporting…" : "Kiro Account Manager Export"}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>

              {/* Maintenance / dangerousActionscollapsible menu */}
              <DropdownMenu modal={false}>
                <DropdownMenuTrigger asChild>
                  <Button
                    size="sm"
                    variant="outline"
                    title="More actions"
                    className="w-full sm:w-auto"
                  >
                    <MoreHorizontal className="h-3.5 w-3.5" />
                    <span className="sm:hidden">More</span>
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuLabel>Bulk actions</DropdownMenuLabel>
                  <DropdownMenuItem
                    onSelect={handleBatchVerify}
                    disabled={selectedIds.size === 0}
                  >
                    <CheckCircle2 />
                    Bulk validate
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onSelect={(e) => {
                      e.preventDefault();
                      handleBatchForceRefresh();
                    }}
                    disabled={selectedIds.size === 0 || batchRefreshing}
                  >
                    <RefreshCw
                      className={batchRefreshing ? "animate-spin" : ""}
                    />
                    {batchRefreshing
                      ? `Refreshing… ${batchRefreshProgress.current}/${batchRefreshProgress.total}`
                      : "Refresh Token"}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onSelect={handleBatchResetFailure}
                    disabled={selectedIds.size === 0}
                  >
                    <RotateCcw />
                    Restore failed
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuLabel>Maintenance</DropdownMenuLabel>
                  <DropdownMenuItem
                    onSelect={(e) => {
                      e.preventDefault();
                      handleQueryCurrentPageInfo();
                    }}
                    disabled={queryingInfo || !data?.credentials?.length}
                  >
                    <RefreshCw className={queryingInfo ? "animate-spin" : ""} />
                    {queryingInfo
                      ? `Refreshing… ${queryInfoProgress.current}/${queryInfoProgress.total}`
                      : "Refresh the balance on the current page"}
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    onSelect={() => setProxyPoolDialogOpen(true)}
                  >
                    <Globe />
                    IP Proxy pool management
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    disabled={
                      resetAllSuccess.isPending || !data?.credentials?.length
                    }
                    onSelect={(e) => {
                      e.preventDefault();
                      resetAllSuccess.mutate(undefined, {
                        onSuccess: (res) => toast.success(res.message),
                        onError: (err) =>
                          toast.error("Reset failed: " + (err as Error).message),
                      });
                    }}
                  >
                    <RotateCcw
                      className={
                        resetAllSuccess.isPending ? "animate-spin" : ""
                      }
                    />
                    Reset success count
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    disabled={
                      enablingOverage ||
                      refreshingOverage ||
                      overageRetryableCount === 0
                    }
                    onSelect={(e) => {
                      e.preventDefault();
                      if (overageEnableableCount > 0) {
                        handleEnableOverageAll();
                      } else {
                        handleRefreshOverageStatus();
                      }
                    }}
                    title={
                      overageRetryableCount === 0
                        ? `All ${overageStats.enabled} items PRO/ENTERPRISE All credentials have overage enabled`
                        : `Enabled ${overageStats.enabled} items / Not enabled ${overageStats.disabledOff} items / Pending ${overageStats.unknown} items`
                    }
                  >
                    <Zap
                      className={
                        enablingOverage || refreshingOverage
                          ? "animate-pulse text-emerald-500"
                          : "text-emerald-500"
                      }
                    />
                    {refreshingOverage
                      ? `Refreshing… ${refreshingOverageProgress.current}/${refreshingOverageProgress.total}`
                      : overageRetryableCount === 0
                        ? `All have overage enabled (${overageStats.enabled})`
                        : overageEnableableCount > 0
                          ? `One-click enable overage (${overageEnableableCount})`
                          : `Retry fetching overage status (${overageStats.unknown})`}
                  </DropdownMenuItem>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    destructive
                    disabled={disablingQuota || quotaExceededCount === 0}
                    onSelect={(e) => {
                      e.preventDefault();
                      handleDisableQuotaExceeded();
                    }}
                  >
                    <AlertTriangle />
                    One-click overage disable ({quotaExceededCount})
                  </DropdownMenuItem>
                  <DropdownMenuItem
                    destructive
                    disabled={disabledCredentialCount === 0}
                    onSelect={(e) => {
                      e.preventDefault();
                      handleClearAll();
                    }}
                  >
                    <Trash2 />
                    Clear disabled ({disabledCredentialCount})
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>
        </div>

        {/* List */}
        {data?.credentials.length === 0 ? (
          <Card>
            <CardContent className="py-16 text-center">
              <div className="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-2xl bg-secondary text-muted-foreground">
                <Server className="h-5 w-5" />
              </div>
              <p className="text-sm text-muted-foreground">
                No credentials yet. Click the top right“Add credential”Start
              </p>
            </CardContent>
          </Card>
        ) : (
          <>
            <DndContext
              sensors={dragSensors}
              collisionDetection={closestCenter}
              onDragEnd={handleDragEnd}
            >
              <SortableContext
                items={currentPageIds}
                strategy={
                  viewMode === "list"
                    ? verticalListSortingStrategy
                    : rectSortingStrategy
                }
              >
                <div
                  className={
                    viewMode === "list"
                      ? "flex select-none flex-col gap-2"
                      : "grid select-none gap-3 sm:gap-4 md:grid-cols-2 lg:grid-cols-3"
                  }
                >
                  {currentCredentials.map((credential) => (
                    <CredentialCard
                      key={credential.id}
                      credential={credential}
                      view={viewMode}
                      selected={selectedIds.has(credential.id)}
                      onToggleSelect={() => toggleSelect(credential.id)}
                      balance={
                        balanceMap.get(credential.id) ||
                        credential.balance ||
                        null
                      }
                      loadingBalance={loadingBalanceIds.has(credential.id)}
                      onRefreshBalance={() =>
                        handleRefreshBalance(credential.id)
                      }
                      failureStats={failureStatsMap?.[String(credential.id)]}
                    />
                  ))}
                </div>
              </SortableContext>
            </DndContext>

            {filteredCredentials.length > 0 && (
              <div className="mt-6 flex flex-col items-center justify-center gap-3 sm:mt-8 sm:flex-row sm:gap-5">
                {/* Per pageCount */}
                <div className="flex items-center gap-2 text-sm text-muted-foreground">
                  <span className="whitespace-nowrap">Per page</span>
                  <Select
                    value={String(pageSize)}
                    onValueChange={(v) => changePageSize(Number(v))}
                  >
                    <SelectTrigger
                      className="h-8 w-[92px] rounded-full border-border bg-card/60 px-3 backdrop-blur"
                      title="Set items shown per page"
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent align="center">
                      {PAGE_SIZE_OPTIONS.map((n) => (
                        <SelectItem key={n} value={String(n)}>
                          {n} items
                        </SelectItem>
                      ))}
                      <SelectItem value="0">All</SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                {/* pagepagecontrol(only multipagetimeShow) */}
                {totalPages > 1 && (
                  <div className="flex flex-wrap items-center justify-center gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setCurrentPage((p) => Math.max(1, p - 1))}
                      disabled={currentPage === 1}
                    >
                      <ChevronLeft className="h-3.5 w-3.5" />
                      Previous page
                    </Button>
                    <div className="order-first w-full px-3 text-center text-sm tabular-nums text-muted-foreground sm:order-none sm:w-auto">
                      No.{" "}
                      <span className="font-medium text-foreground">
                        {currentPage}
                      </span>{" "}
                      / {totalPages} page
                      <span className="mx-1.5 text-muted-foreground/50">·</span>
                      total {filteredCredentials.length} items
                    </div>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() =>
                        setCurrentPage((p) => Math.min(totalPages, p + 1))
                      }
                      disabled={currentPage === totalPages}
                    >
                      Next page
                      <ChevronRight className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                )}
              </div>
            )}
          </>
        )}
      </main>

      {/* the dialogs */}
      <AddCredentialDialog
        open={addDialogOpen}
        onOpenChange={setAddDialogOpen}
      />
      <BatchImportDialog
        open={batchImportDialogOpen}
        onOpenChange={setBatchImportDialogOpen}
      />
      <BatchEditCredentialDialog
        open={batchEditDialogOpen}
        onOpenChange={setBatchEditDialogOpen}
        credentials={(data?.credentials ?? []).filter((c) =>
          selectedIds.has(c.id),
        )}
        groupOptions={groupOptions}
        onDone={deselectAll}
      />
      <SocialLoginDialog
        open={socialLoginDialogOpen}
        onOpenChange={setSocialLoginDialogOpen}
        onSuccess={() =>
          queryClient.invalidateQueries({ queryKey: ["credentials"] })
        }
      />
      <IdcLoginDialog
        open={idcLoginDialogOpen}
        onOpenChange={setIdcLoginDialogOpen}
        onSuccess={() =>
          queryClient.invalidateQueries({ queryKey: ["credentials"] })
        }
      />
      <IdcLoginDialog
        mode="enterprise"
        open={enterpriseLoginDialogOpen}
        onOpenChange={setEnterpriseLoginDialogOpen}
        onSuccess={() =>
          queryClient.invalidateQueries({ queryKey: ["credentials"] })
        }
      />
      <KamImportDialog
        open={kamImportDialogOpen}
        onOpenChange={setKamImportDialogOpen}
      />
      <ProxyPoolDialog
        open={proxyPoolDialogOpen}
        onOpenChange={setProxyPoolDialogOpen}
      />
      <ImageUpdateDialog
        open={imageUpdateDialogOpen}
        onOpenChange={setImageUpdateDialogOpen}
      />

      {/* Change loginAPIKeydialog(adminApiKey —— ManagepanelLoginKey) */}
      <Dialog
        open={adminKeyDialogOpen}
        onOpenChange={(open) => {
          if (!updatingAdminKey) setAdminKeyDialogOpen(open);
        }}
      >
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Key className="h-4 w-4" />
              Change loginAPIKey
            </DialogTitle>
            <DialogDescription>
              Used to log in to this admin panel. After changing it, the locally stored value is updated automatically Key, no need to log in again.
            </DialogDescription>
          </DialogHeader>
          <form onSubmit={handleUpdateAdminKey} className="space-y-4 py-2">
            <div className="relative">
              <Input
                type={showAdminKeyPlain ? "text" : "password"}
                placeholder="Enter or generate a new loginAPIKey"
                value={newAdminKey}
                onChange={(e) => setNewAdminKey(e.target.value)}
                disabled={updatingAdminKey}
                autoFocus
                className="pr-20 font-mono text-[13px]"
              />
              <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-1.5">
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  className="pointer-events-auto h-7 w-7"
                  onClick={() => setShowAdminKeyPlain((v) => !v)}
                  disabled={updatingAdminKey}
                  title={showAdminKeyPlain ? "Hide" : "Show"}
                >
                  {showAdminKeyPlain ? (
                    <EyeOff className="h-3.5 w-3.5" />
                  ) : (
                    <Eye className="h-3.5 w-3.5" />
                  )}
                </Button>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  className="pointer-events-auto h-7 w-7"
                  onClick={async () => {
                    if (!newAdminKey.trim()) {
                      toast.error("Please enter or generate first Key then copy");
                      return;
                    }
                    try {
                      await navigator.clipboard.writeText(newAdminKey);
                      toast.success("Copied to clipboard");
                    } catch {
                      toast.error("Copy failed. Please select the text manually");
                    }
                  }}
                  disabled={updatingAdminKey}
                  title="Copy"
                >
                  <Copy className="h-3.5 w-3.5" />
                </Button>
              </div>
            </div>
            <div className="flex items-center justify-between gap-2">
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => {
                  const key = generateApiKey("sk-admin-");
                  setNewAdminKey(key);
                  setShowAdminKeyPlain(true);
                }}
                disabled={updatingAdminKey}
              >
                <Wand2 className="h-3.5 w-3.5" />
                Generate random Key
              </Button>
              <p className="text-[11px] text-muted-foreground">
                It is recommended to copy and save it right after generating; it takes effect once the update is confirmed.
              </p>
            </div>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => setAdminKeyDialogOpen(false)}
                disabled={updatingAdminKey}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                disabled={updatingAdminKey || !newAdminKey.trim()}
              >
                {updatingAdminKey ? "Updating…" : "Confirm update"}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      {rectSelection.active && rectSelection.rect && (
        <div
          className="pointer-events-none fixed z-50 rounded-sm border border-primary/70 bg-primary/15"
          style={{
            left: rectSelection.rect.left,
            top: rectSelection.rect.top,
            width: rectSelection.rect.width,
            height: rectSelection.rect.height,
          }}
        />
      )}
      <BatchVerifyDialog
        open={verifyDialogOpen}
        onOpenChange={setVerifyDialogOpen}
        verifying={verifying}
        progress={verifyProgress}
        results={verifyResults}
        onCancel={handleCancelVerify}
        onDelete={handleDeleteVerifyResult}
        onDeleteFailed={handleDeleteFailedVerify}
        deleting={verifyDeleting}
      />
    </div>
  );
}
