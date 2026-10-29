<script setup>
import { ref, computed, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { useAccount } from 'dashboard/composables/useAccount';
import { useStore, useMapGetter } from 'dashboard/composables/store';
import { useAlert } from 'dashboard/composables';
import Icon from 'dashboard/components-next/icon/Icon.vue';
import Button from 'dashboard/components-next/button/Button.vue';
import Spinner from 'dashboard/components-next/spinner/Spinner.vue';

const { t } = useI18n();
const router = useRouter();
const store = useStore();
const { accountId, currentAccount } = useAccount();
const currentUser = useMapGetter('getCurrentUser');

const isLoading = ref(true);
const isSubmitting = ref(false);
const activeTierId = ref('free');
const isRegistered = ref(false);
const availableTiers = ref([]);

const formatCurrency = price => {
  if (price === 0) return 'Rp 0';
  return `Rp ${price.toLocaleString('id-ID')}`;
};

const activeTier = computed(() => {
  return (
    currentAccount.value?.custom_attributes?.plan_name ||
    currentAccount.value?.custom_attributes?.tier ||
    activeTierId.value
  );
});

const fetchTiersData = async () => {
  isLoading.value = true;
  try {
    const data = await store.dispatch('accounts/getTier', accountId.value);
    if (data) {
      availableTiers.value = data.available_tiers || [];
      activeTierId.value = data.current_tier || 'free';
      isRegistered.value = Boolean(data.is_registered);
    }
  } catch (_) {
    // Fallback catalog if backend offline
    availableTiers.value = [
      {
        id: 'free',
        name: 'Free',
        badge: t('BILLING_SETTINGS.TIER_SELECTION.FREE_BADGE'),
        description:
          'Untuk pemula & startup yang membutuhkan live chat dan email dasar.',
        price_monthly: 0,
        agent_limit: 2,
        inbox_limit: 2,
        highlight_features: [
          '2 Agen & 2 Inbox',
          'Live Chat Widget & Email',
          'Canned Responses & Labels',
          'Laporan Dasar',
        ],
      },
      {
        id: 'small',
        name: 'Small',
        badge: t('BILLING_SETTINGS.TIER_SELECTION.POPULAR_BADGE'),
        description:
          'Untuk tim kecil dengan WhatsApp Cloud API & Omnichannel terintegrasi.',
        price_monthly: 299000,
        agent_limit: 5,
        inbox_limit: 5,
        highlight_features: [
          '5 Agen & 5 Inbox',
          'WhatsApp Cloud API Resmi',
          'Instagram Direct & Telegram',
          'Macro & Basic Automations',
        ],
      },
      {
        id: 'max',
        name: 'Max',
        badge: t('BILLING_SETTINGS.TIER_SELECTION.ADVANCED_BADGE'),
        description:
          'Untuk bisnis bertumbuh dengan volume chat tinggi, SLA & AI assist.',
        price_monthly: 799000,
        agent_limit: 15,
        inbox_limit: 20,
        highlight_features: [
          '15 Agen & 20 Inbox',
          'Semua Channel + Shopee',
          'SLA Management & CSAT',
          'Conversation Monitors & Auto Assignment',
          'Captain AI Bot Integration',
        ],
      },
      {
        id: 'enterprise',
        name: 'Enterprise (Chat Admin)',
        badge: t('BILLING_SETTINGS.TIER_SELECTION.ENTERPRISE_BADGE'),
        description:
          'Skala korporasi dengan Chat Admin, Custom Roles, Audit Logs & Unlimited.',
        price_monthly: 1999000,
        agent_limit: -1,
        inbox_limit: -1,
        highlight_features: [
          'Unlimited Agen & Inbox',
          'Dedicated Chat Admin Governance',
          'Custom Roles & Granular Permissions',
          'Audit Logs & Security Compliance',
          'Captain AI Full Suite & Custom Tools',
          'SAML SSO & Prioritas Webhook Routing',
        ],
      },
    ];
  } finally {
    isLoading.value = false;
  }
};

const handleSelectTier = async tier => {
  if (isSubmitting.value) return;
  isSubmitting.value = true;

  try {
    await store.dispatch('accounts/setTier', {
      tier: tier.id,
      accountId: accountId.value,
    });
    activeTierId.value = tier.id;
    isRegistered.value = true;
    useAlert(
      t('BILLING_SETTINGS.TIER_SELECTION.SWITCH_SUCCESS', {
        tier: tier.name,
      })
    );
    router.replace({
      name: 'home',
      params: { accountId: accountId.value },
    });
  } catch (_) {
    useAlert(t('BILLING_SETTINGS.TIER_SELECTION.SWITCH_ERROR'));
  } finally {
    isSubmitting.value = false;
  }
};

const handleContinueToDashboard = () => {
  router.replace({
    name: 'home',
    params: { accountId: accountId.value },
  });
};

onMounted(() => {
  fetchTiersData();
});
</script>

<template>
  <div class="min-h-screen bg-n-slate-2 flex flex-col p-6 lg:p-12">
    <!-- Header Bar -->
    <div
      class="max-w-7xl mx-auto w-full flex items-center justify-between pb-8 mb-4 border-b border-n-weak"
    >
      <div class="flex items-center gap-3">
        <div
          class="size-10 rounded-xl bg-n-brand/10 text-n-brand flex items-center justify-center font-bold text-lg"
        >
          <span>{{
            $t('BILLING_SETTINGS.TIER_SELECTION.BRAND_INITIALS')
          }}</span>
        </div>
        <div>
          <h2 class="text-base font-semibold text-n-slate-12">
            {{ currentAccount.name || 'Chatwoot' }}
          </h2>
          <p class="text-xs text-n-slate-11">
            {{ currentUser.email }}
          </p>
        </div>
      </div>

      <Button
        v-if="isRegistered"
        outline
        slate
        size="sm"
        icon="i-lucide-arrow-right"
        trailing-icon
        :label="$t('BILLING_SETTINGS.TIER_SELECTION.CONTINUE_TO_DASHBOARD')"
        @click="handleContinueToDashboard"
      />
    </div>

    <!-- Title & Description -->
    <div class="max-w-4xl mx-auto text-center mb-10">
      <div
        class="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-n-alpha-2 text-n-slate-12 text-xs font-medium mb-3"
      >
        <Icon icon="i-lucide-sparkles" class="size-3.5 text-n-brand" />
        <span>{{ $t('BILLING_SETTINGS.TIER_SELECTION.TITLE') }}</span>
      </div>
      <h1 class="text-2xl lg:text-3xl font-bold text-n-slate-12 mb-3">
        {{ $t('BILLING_SETTINGS.TIER_SELECTION.TITLE') }}
      </h1>
      <p
        class="text-sm lg:text-base text-n-slate-11 leading-relaxed max-w-2xl mx-auto"
      >
        {{ $t('BILLING_SETTINGS.TIER_SELECTION.SUBTITLE') }}
      </p>
    </div>

    <!-- Loading State -->
    <div v-if="isLoading" class="flex-1 flex items-center justify-center py-20">
      <Spinner size="lg" />
    </div>

    <!-- Tier Cards Grid -->
    <div
      v-else
      class="max-w-7xl mx-auto w-full grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 items-stretch"
    >
      <div
        v-for="tier in availableTiers"
        :key="tier.id"
        class="relative flex flex-col justify-between rounded-2xl border bg-n-solid-1 p-6 transition-all duration-200 hover:shadow-lg"
        :class="[
          tier.id === 'enterprise'
            ? 'border-amber-400/80 shadow-amber-500/5 ring-1 ring-amber-400/30'
            : tier.id === activeTier
              ? 'border-n-brand ring-1 ring-n-brand shadow-sm'
              : 'border-n-weak hover:border-n-slate-6',
        ]"
      >
        <!-- Top Badge -->
        <div class="flex items-center justify-between mb-4">
          <span
            class="px-2.5 py-0.5 rounded-full text-xs font-semibold tracking-wide"
            :class="[
              tier.id === 'enterprise'
                ? 'bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300'
                : tier.id === 'max'
                  ? 'bg-purple-100 text-purple-800 dark:bg-purple-950 dark:text-purple-300'
                  : tier.id === 'small'
                    ? 'bg-blue-100 text-blue-800 dark:bg-blue-950 dark:text-blue-300'
                    : 'bg-n-slate-3 text-n-slate-11',
            ]"
          >
            {{ tier.badge }}
          </span>

          <span
            v-if="tier.id === activeTier"
            class="flex items-center gap-1 text-xs font-medium text-emerald-600 dark:text-emerald-400"
          >
            <Icon icon="i-lucide-check-circle" class="size-4" />
            <span>{{ $t('BILLING_SETTINGS.TIER_SELECTION.ACTIVE') }}</span>
          </span>
        </div>

        <!-- Plan Info -->
        <div class="mb-6">
          <h3 class="text-xl font-bold text-n-slate-12 mb-2">
            {{ tier.name }}
          </h3>
          <p class="text-xs text-n-slate-11 leading-relaxed min-h-[36px]">
            {{ tier.description }}
          </p>

          <div class="mt-4 pb-4 border-b border-n-weak">
            <div class="flex items-baseline gap-1">
              <span class="text-2xl font-extrabold text-n-slate-12">
                {{ formatCurrency(tier.price_monthly) }}
              </span>
              <span class="text-xs text-n-slate-10">
                {{ $t('BILLING_SETTINGS.TIER_SELECTION.PER_MONTH') }}
              </span>
            </div>
            <div class="mt-2 flex items-center gap-3 text-xs text-n-slate-11">
              <span class="flex items-center gap-1">
                <Icon icon="i-lucide-users" class="size-3.5 text-n-slate-9" />
                {{
                  tier.agent_limit === -1
                    ? $t('BILLING_SETTINGS.TIER_SELECTION.UNLIMITED')
                    : `${tier.agent_limit} ${$t('BILLING_SETTINGS.TIER_SELECTION.AGENTS')}`
                }}
              </span>
              <span
                class="size-1 rounded-full bg-n-slate-8 inline-block flex-shrink-0"
              />
              <span class="flex items-center gap-1">
                <Icon icon="i-lucide-inbox" class="size-3.5 text-n-slate-9" />
                {{
                  tier.inbox_limit === -1
                    ? $t('BILLING_SETTINGS.TIER_SELECTION.UNLIMITED')
                    : `${tier.inbox_limit} ${$t('BILLING_SETTINGS.TIER_SELECTION.INBOXES')}`
                }}
              </span>
            </div>
          </div>
        </div>

        <!-- Features Bullets -->
        <div class="flex-1 mb-8">
          <ul class="space-y-2.5">
            <li
              v-for="(feature, idx) in tier.highlight_features"
              :key="idx"
              class="flex items-start gap-2.5 text-xs text-n-slate-12"
            >
              <Icon
                icon="i-lucide-check"
                class="size-4 flex-shrink-0 mt-0.5"
                :class="
                  tier.id === 'enterprise'
                    ? 'text-amber-500'
                    : 'text-emerald-500'
                "
              />
              <span>{{ feature }}</span>
            </li>
          </ul>
        </div>

        <!-- Action Button -->
        <div>
          <Button
            class="w-full"
            :solid="tier.id === 'enterprise' || tier.id === 'max'"
            :outline="tier.id === 'free' || tier.id === 'small'"
            :color="tier.id === 'enterprise' ? 'amber' : 'brand'"
            :is-loading="isSubmitting"
            :disabled="isSubmitting || tier.id === activeTier"
            :label="
              tier.id === activeTier
                ? $t('BILLING_SETTINGS.TIER_SELECTION.CURRENT_PLAN')
                : $t('BILLING_SETTINGS.TIER_SELECTION.SELECT_PLAN')
            "
            @click="handleSelectTier(tier)"
          />
        </div>
      </div>
    </div>
  </div>
</template>
