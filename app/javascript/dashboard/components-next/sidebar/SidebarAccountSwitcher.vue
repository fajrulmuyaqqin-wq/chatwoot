<script setup>
import { computed } from 'vue';
import { useRouter } from 'vue-router';
import { useAccount } from 'dashboard/composables/useAccount';
import { useMapGetter } from 'dashboard/composables/store';
import { useI18n } from 'vue-i18n';
import { useAdmin } from 'dashboard/composables/useAdmin';
import { useAlert } from 'dashboard/composables';
import ButtonNext from 'next/button/Button.vue';
import Icon from 'next/icon/Icon.vue';
import Logo from 'next/icon/Logo.vue';

import {
  DropdownContainer,
  DropdownBody,
  DropdownSection,
  DropdownItem,
} from 'next/dropdown-menu/base';

defineProps({
  isCollapsed: {
    type: Boolean,
    default: false,
  },
});

const emit = defineEmits(['showCreateAccountModal']);

const { t } = useI18n();
const router = useRouter();
const { accountId, currentAccount } = useAccount();
const { isAdmin } = useAdmin();
const currentUser = useMapGetter('getCurrentUser');
const globalConfig = useMapGetter('globalConfig/get');

const userAccounts = useMapGetter('getUserAccounts');

const isSuperAdmin = computed(() => currentUser.value?.type === 'SuperAdmin');

const canCreateNewAccount = computed(() => {
  // Multi-tenant safety: each tenant user is restricted to 1 account, unless SuperAdmin
  if (isSuperAdmin.value) {
    return Boolean(globalConfig.createNewAccountFromDashboard);
  }
  return (
    Boolean(globalConfig.createNewAccountFromDashboard) &&
    userAccounts.value.length === 0
  );
});

const showAccountSwitcher = computed(
  () => userAccounts.value.length > 1 && currentAccount.value.name
);

const canToggleDropdown = computed(
  () => showAccountSwitcher.value || isAdmin.value || canCreateNewAccount.value
);

const sortedCurrentUserAccounts = computed(() => {
  return [...(currentUser.value.accounts || [])].sort((a, b) =>
    a.name.localeCompare(b.name)
  );
});

const currentPlan = computed(() => {
  return (
    currentAccount.value?.custom_attributes?.plan_name ||
    currentAccount.value?.custom_attributes?.tier ||
    'free'
  );
});

const tierBadgeConfig = computed(() => {
  const plan = currentPlan.value.toLowerCase();
  switch (plan) {
    case 'enterprise':
      return {
        label: 'Chat Admin',
        icon: 'i-lucide-shield-check',
        classes:
          'bg-amber-500/10 text-amber-700 dark:text-amber-400 border border-amber-500/30',
      };
    case 'max':
      return {
        label: 'Max',
        icon: 'i-lucide-zap',
        classes:
          'bg-purple-500/10 text-purple-700 dark:text-purple-400 border border-purple-500/30',
      };
    case 'small':
      return {
        label: 'Small',
        icon: 'i-lucide-sparkles',
        classes:
          'bg-blue-500/10 text-blue-700 dark:text-blue-400 border border-blue-500/30',
      };
    case 'free':
    default:
      return {
        label: 'Free',
        icon: null,
        classes: 'bg-n-slate-3 text-n-slate-11 border border-n-weak',
      };
  }
});

const getAccountTierLabel = planName => {
  if (!planName) return 'Free';
  const p = planName.toLowerCase();
  if (p === 'enterprise') return 'Chat Admin';
  if (p === 'max') return 'Max';
  if (p === 'small') return 'Small';
  return 'Free';
};

const getAccountTierClasses = planName => {
  const p = (planName || 'free').toLowerCase();
  if (p === 'enterprise') {
    return 'bg-amber-500/10 text-amber-700 dark:text-amber-400 border border-amber-500/30';
  }
  if (p === 'max') {
    return 'bg-purple-500/10 text-purple-700 dark:text-purple-400 border border-purple-500/30';
  }
  if (p === 'small') {
    return 'bg-blue-500/10 text-blue-700 dark:text-blue-400 border border-blue-500/30';
  }
  return 'bg-n-slate-3 text-n-slate-11 border border-n-weak';
};

const onChangeAccount = newId => {
  const accountUrl = `/app/accounts/${newId}/dashboard`;
  window.location.href = accountUrl;
};

const navigateToPlanSelection = () => {
  router.push({
    name: 'plan_selection',
    params: { accountId: accountId.value },
  });
};

const onBadgeClick = () => {
  if (isAdmin.value) {
    navigateToPlanSelection();
  }
};

const emitNewAccount = () => {
  if (!canCreateNewAccount.value) {
    useAlert(
      t('CREATE_ACCOUNT.SAFETY_SINGLE_ACCOUNT_LIMIT') ||
        'Each user is restricted to 1 account in multi-tenant mode.'
    );
    return;
  }
  emit('showCreateAccountModal');
};
</script>

<template>
  <DropdownContainer>
    <template #trigger="{ toggle, isOpen }">
      <!-- Collapsed view: Logo trigger -->
      <button
        v-if="isCollapsed"
        class="grid flex-shrink-0 place-content-center p-2 rounded-lg cursor-pointer hover:bg-n-alpha-1"
        :class="{ 'bg-n-alpha-1': isOpen }"
        :title="currentAccount.name"
        @click="toggle"
      >
        <Logo class="size-7" />
      </button>
      <!-- Expanded view: Account name trigger -->
      <button
        v-else
        id="sidebar-account-switcher"
        :data-account-id="accountId"
        aria-haspopup="listbox"
        aria-controls="account-options"
        class="flex items-center gap-2 justify-between w-full rounded-lg px-2 py-1 text-start"
        :class="[
          isOpen && 'bg-n-alpha-1',
          canToggleDropdown
            ? 'hover:bg-n-alpha-1 cursor-pointer'
            : 'cursor-default',
        ]"
        @click="() => canToggleDropdown && toggle()"
      >
        <div class="flex flex-col items-start min-w-0 flex-grow">
          <div class="flex items-center gap-1.5 w-full">
            <span
              class="text-sm font-medium leading-5 text-n-slate-12 truncate"
              aria-live="polite"
            >
              {{ currentAccount.name }}
            </span>
            <span
              v-if="canToggleDropdown"
              aria-hidden="true"
              class="i-lucide-chevron-down size-3.5 text-n-slate-10 flex-shrink-0"
            />
          </div>
          <span
            role="button"
            tabindex="0"
            class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-semibold tracking-wide uppercase mt-0.5 transition-all duration-150"
            :class="[
              tierBadgeConfig.classes,
              isAdmin
                ? 'cursor-pointer hover:brightness-95 active:scale-95'
                : 'cursor-default',
            ]"
            :title="
              isAdmin
                ? t('SIDEBAR_ITEMS.MANAGE_SERVICE_PLAN', {
                    tier: tierBadgeConfig.label,
                  })
                : undefined
            "
            @click.stop="onBadgeClick"
            @keydown.enter.stop="onBadgeClick"
          >
            <span
              v-if="tierBadgeConfig.icon"
              class="size-3 flex-shrink-0"
              :class="tierBadgeConfig.icon"
            />
            {{ tierBadgeConfig.label }}
          </span>
        </div>
      </button>
    </template>
    <DropdownBody v-if="canToggleDropdown || isCollapsed" class="min-w-80 z-50">
      <DropdownSection
        v-if="showAccountSwitcher"
        :title="t('SIDEBAR_ITEMS.SWITCH_ACCOUNT')"
      >
        <DropdownItem
          v-for="account in sortedCurrentUserAccounts"
          :id="`account-${account.id}`"
          :key="account.id"
          class="cursor-pointer"
          @click="onChangeAccount(account.id)"
        >
          <template #label>
            <div
              :for="account.name"
              class="text-left rtl:text-right flex gap-2 items-center w-full"
            >
              <span
                class="text-n-slate-12 max-w-28 truncate min-w-0"
                :title="account.name"
              >
                {{ account.name }}
              </span>
              <div class="flex-shrink-0 w-px h-3 bg-n-strong" />
              <span
                class="text-n-slate-11 text-xs truncate capitalize"
                :title="account.role"
              >
                {{
                  account.custom_role_id
                    ? account.custom_role.name
                    : account.role
                }}
              </span>
              <span
                class="ms-auto text-[10px] px-1.5 py-0.5 rounded font-semibold uppercase flex-shrink-0"
                :class="
                  getAccountTierClasses(account.custom_attributes?.plan_name)
                "
              >
                {{ getAccountTierLabel(account.custom_attributes?.plan_name) }}
              </span>
            </div>
            <Icon
              v-show="account.id === accountId"
              icon="i-lucide-check"
              class="text-n-teal-11 size-4 flex-shrink-0 ms-1"
            />
          </template>
        </DropdownItem>
      </DropdownSection>
      <DropdownSection v-if="isAdmin" :title="t('SIDEBAR_ITEMS.SERVICE_PLAN')">
        <DropdownItem class="cursor-pointer" @click="navigateToPlanSelection">
          <template #label>
            <div
              class="flex items-center gap-2 text-n-brand text-xs font-medium py-0.5"
            >
              <span class="i-lucide-sparkles size-3.5 flex-shrink-0" />
              <span>
                {{
                  t('SIDEBAR_ITEMS.MANAGE_SERVICE_PLAN', {
                    tier: tierBadgeConfig.label,
                  })
                }}
              </span>
            </div>
          </template>
        </DropdownItem>
      </DropdownSection>
      <DropdownItem v-if="canCreateNewAccount">
        <ButtonNext
          color="slate"
          variant="faded"
          class="w-full"
          size="sm"
          @click="emitNewAccount"
        >
          {{ t('CREATE_ACCOUNT.NEW_ACCOUNT') }}
        </ButtonNext>
      </DropdownItem>
    </DropdownBody>
  </DropdownContainer>
</template>
