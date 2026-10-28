<script>
import { useVuelidate } from '@vuelidate/core';
import { useAlert } from 'dashboard/composables';
import { useFacebookPageConnect } from 'dashboard/composables/useFacebookPageConnect';
import { required } from '@vuelidate/validators';
import LoadingState from 'dashboard/components/widgets/LoadingState.vue';
import Banner from 'dashboard/components-next/banner/Banner.vue';
import Icon from 'dashboard/components-next/icon/Icon.vue';

import PageHeader from '../../SettingsSubPageHeader.vue';
import router from '../../../../index';
import { useBranding } from 'shared/composables/useBranding';
import { useAccount } from 'dashboard/composables/useAccount';
import NextButton from 'dashboard/components-next/button/Button.vue';
import ComboBox from 'dashboard/components-next/combobox/ComboBox.vue';
import { META_RESTRICTION_STATUS_URL } from 'dashboard/constants/globals';
import { parseAPIErrorResponse } from 'dashboard/store/utils/api';

import * as Sentry from '@sentry/vue';

export default {
  components: {
    LoadingState,
    PageHeader,
    NextButton,
    ComboBox,
    Banner,
    Icon,
  },
  setup() {
    const { replaceInstallationName } = useBranding();
    const { isMetaInboxCreationDisabled } = useAccount();
    const { preloadSdk, loginAndFetchPages } = useFacebookPageConnect();
    return {
      replaceInstallationName,
      isMetaInboxCreationDisabled,
      preloadSdk,
      loginAndFetchPages,
      META_RESTRICTION_STATUS_URL,
      v$: useVuelidate(),
    };
  },
  data() {
    return {
      isCreating: false,
      hasError: false,
      user_access_token: '',
      selectedPage: { name: null, id: null },
      pageName: '',
      pageList: [],
      emptyStateMessage: this.$t('INBOX_MGMT.DETAILS.LOADING_FB'),
      errorStateMessage: '',
      errorStateDescription: '',
      hasLoginStarted: false,
    };
  },

  validations: {
    pageName: {
      required,
    },

    selectedPage: {
      isEmpty() {
        return this.selectedPage !== null && !!this.selectedPage.name;
      },
    },
  },

  computed: {
    showLoader() {
      return !this.user_access_token || this.isCreating;
    },
    getSelectablePages() {
      return this.pageList.filter(item => !item.exists);
    },
    comboBoxPageOptions() {
      return this.getSelectablePages.map(({ id, name }) => ({
        value: id,
        label: name,
      }));
    },
    isFacebookConnectionDisabled() {
      return this.isMetaInboxCreationDisabled;
    },
    isHttpProtocol() {
      return (
        typeof window !== 'undefined' && window.location.protocol === 'http:'
      );
    },
  },

  mounted() {
    // Warm the SDK so the login click opens its popup within the gesture's
    // activation window (see useFacebookPageConnect).
    if (!this.isFacebookConnectionDisabled) {
      this.preloadSdk();
    }
  },

  methods: {
    async startLogin() {
      if (this.isFacebookConnectionDisabled) return;

      this.hasLoginStarted = true;
      try {
        const result = await this.loginAndFetchPages();
        if (!result) {
          // Cancelled popup / not authorized — surface a generic auth error.
          this.hasError = true;
          this.errorStateMessage = this.$t('INBOX_MGMT.DETAILS.ERROR_FB_AUTH');
          this.errorStateDescription = this.isHttpProtocol
            ? this.$t('INBOX_MGMT.ADD.FB.ERROR_HTTP_DESC')
            : this.$t('INBOX_MGMT.ADD.FB.ERROR_CONFIG_DESC');
          return;
        }
        this.pageList = result.pages;
        this.user_access_token = result.userAccessToken;
      } catch (error) {
        this.hasError = true;
        this.errorStateMessage = this.$t('INBOX_MGMT.DETAILS.ERROR_FB_AUTH');
        this.errorStateDescription = this.isHttpProtocol
          ? this.$t('INBOX_MGMT.ADD.FB.ERROR_HTTP_DESC')
          : '';
        if (error.name === 'ScriptLoaderError') {
          useAlert(this.$t('INBOX_MGMT.DETAILS.ERROR_FB_LOADING'));
        } else {
          Sentry.captureException(error);
          useAlert(this.$t('INBOX_MGMT.DETAILS.ERROR_FB_AUTH'));
        }
      }
    },

    resetLogin() {
      this.hasLoginStarted = false;
      this.hasError = false;
      this.errorStateMessage = '';
      this.errorStateDescription = '';
    },

    setPageName(pageId) {
      const page = this.pageList.find(p => p.id === pageId);
      if (page) {
        this.selectedPage = page;
        this.pageName = page.name;
      } else {
        this.selectedPage = { name: null, id: null };
        this.pageName = '';
      }
      this.v$.selectedPage.$touch();
    },

    channelParams() {
      return {
        user_access_token: this.user_access_token,
        page_access_token: this.selectedPage.access_token,
        page_id: this.selectedPage.id,
        inbox_name: this.selectedPage.name?.trim(),
      };
    },

    createChannel() {
      this.v$.$touch();
      if (!this.v$.$error) {
        this.emptyStateMessage = this.$t('INBOX_MGMT.DETAILS.CREATING_CHANNEL');
        this.isCreating = true;
        this.$store
          .dispatch('inboxes/createFBChannel', this.channelParams())
          .then(data => {
            router.replace({
              name: 'settings_inboxes_add_agents',
              params: { page: 'new', inbox_id: data.id },
            });
          })
          .catch(error => {
            const errorMessage = parseAPIErrorResponse(error);
            useAlert(
              typeof errorMessage === 'string'
                ? errorMessage
                : this.$t('INBOX_MGMT.DETAILS.ERROR_CHANNEL_CREATION')
            );
            this.isCreating = false;
          });
      }
    },
  },
};
</script>

<template>
  <div class="w-full h-full col-span-6 p-6 overflow-auto">
    <div
      v-if="!hasLoginStarted"
      class="flex flex-col items-center justify-center h-full text-center"
    >
      <div class="flex flex-col items-center w-full max-w-2xl">
        <button
          type="button"
          :disabled="isFacebookConnectionDisabled"
          :class="{
            'cursor-not-allowed opacity-50': isFacebookConnectionDisabled,
          }"
          @click="startLogin()"
        >
          <img
            class="w-auto h-10 rounded-md"
            src="~dashboard/assets/images/channels/facebook_login.png"
            alt="Facebook-logo"
          />
        </button>
        <p class="py-6">
          {{ replaceInstallationName($t('INBOX_MGMT.ADD.FB.HELP')) }}
        </p>
        <Banner
          v-if="isHttpProtocol"
          color="amber"
          class="w-full mb-6 text-start"
        >
          <div class="flex items-start gap-3">
            <Icon
              icon="i-lucide-triangle-alert"
              class="flex-shrink-0 size-4 mt-0.5 text-n-amber-11"
            />
            <div class="text-xs space-y-1">
              <p class="font-medium text-n-amber-12">
                {{ $t('INBOX_MGMT.ADD.FB.HTTPS_REQUIRED_TITLE') }}
              </p>
              <p>
                {{ $t('INBOX_MGMT.ADD.FB.HTTPS_REQUIRED_DESC') }}
              </p>
            </div>
          </div>
        </Banner>
        <Banner
          v-if="isFacebookConnectionDisabled"
          color="amber"
          class="w-full"
        >
          <div class="flex items-start gap-3 text-start">
            <Icon
              icon="i-lucide-triangle-alert"
              class="flex-shrink-0 size-4 mt-0.5"
            />
            <span>
              {{ $t('INBOX_MGMT.ADD.FB.RESTRICTED_WARNING') }}
              <a
                :href="META_RESTRICTION_STATUS_URL"
                class="link underline"
                rel="noopener noreferrer nofollow"
                target="_blank"
              >
                {{ $t('INBOX_MGMT.ADD.FB.STATUS_LINK') }}
              </a>
            </span>
          </div>
        </Banner>
        <div
          class="w-full mt-4 p-4 text-start bg-n-slate-2 border border-n-slate-4 rounded-xl"
        >
          <h4
            class="text-sm font-semibold text-n-slate-12 mb-3 flex items-center gap-2"
          >
            <Icon icon="i-lucide-info" class="size-4 text-n-brand" />
            {{ $t('INBOX_MGMT.ADD.FB.REQUIREMENTS_TITLE') }}
          </h4>
          <ul class="text-xs text-n-slate-11 space-y-2.5 list-disc list-inside">
            <li>
              <span class="font-medium text-n-slate-12">
                {{ $t('INBOX_MGMT.ADD.FB.REQ_META_APP_TITLE') }}
              </span>
              <span>
                {{ $t('INBOX_MGMT.ADD.FB.REQ_META_APP_DESC') }}
              </span>
              <a
                href="https://developers.facebook.com/"
                target="_blank"
                rel="noopener noreferrer"
                class="link underline text-n-brand ml-1"
              >
                {{ $t('INBOX_MGMT.ADD.FB.META_DEVELOPERS_LINK') }}
              </a>
            </li>
            <li>
              <span class="font-medium text-n-slate-12">
                {{ $t('INBOX_MGMT.ADD.FB.REQ_PRODUCTS_TITLE') }}
              </span>
              <span>{{ $t('INBOX_MGMT.ADD.FB.REQ_PRODUCTS_DESC') }}</span>
            </li>
            <li>
              <span class="font-medium text-n-slate-12">
                {{ $t('INBOX_MGMT.ADD.FB.REQ_HTTPS_TITLE') }}
              </span>
              <span>{{ $t('INBOX_MGMT.ADD.FB.REQ_HTTPS_DESC') }}</span>
            </li>
            <li>
              <span class="font-medium text-n-slate-12">
                {{ $t('INBOX_MGMT.ADD.FB.REQ_ENV_TITLE') }}
              </span>
              <span>{{ $t('INBOX_MGMT.ADD.FB.REQ_ENV_DESC') }}</span>
            </li>
          </ul>
        </div>
      </div>
    </div>
    <div v-else>
      <div v-if="hasError" class="max-w-lg mx-auto text-center space-y-4">
        <h5>{{ errorStateMessage }}</h5>
        <p
          v-if="errorStateDescription"
          v-dompurify-html="errorStateDescription"
        />
        <div class="pt-2">
          <NextButton
            :label="$t('INBOX_MGMT.ADD.FB.TRY_AGAIN')"
            @click="resetLogin()"
          />
        </div>
      </div>
      <LoadingState v-else-if="showLoader" :message="emptyStateMessage" />
      <form
        v-else
        class="flex flex-col flex-wrap mx-0"
        @submit.prevent="createChannel()"
      >
        <div class="w-full">
          <PageHeader
            :header-title="$t('INBOX_MGMT.ADD.DETAILS.TITLE')"
            :header-content="
              replaceInstallationName($t('INBOX_MGMT.ADD.DETAILS.DESC'))
            "
          />
        </div>
        <div class="w-3/5">
          <div class="w-full mb-2">
            <div class="input-wrap" :class="{ error: v$.selectedPage.$error }">
              <span class="text-n-slate-12 text-start">
                {{ $t('INBOX_MGMT.ADD.FB.CHOOSE_PAGE') }}
              </span>
              <ComboBox
                :model-value="selectedPage.id"
                :options="comboBoxPageOptions"
                :placeholder="$t('INBOX_MGMT.ADD.FB.PICK_A_VALUE')"
                :has-error="v$.selectedPage.$error"
                class="[&>div>button]:!bg-n-alpha-black2 mt-1"
                @update:model-value="setPageName"
              />
              <span v-if="v$.selectedPage.$error" class="message mt-0.5">
                {{ $t('INBOX_MGMT.ADD.FB.CHOOSE_PLACEHOLDER') }}
              </span>
            </div>
          </div>
          <div class="w-full">
            <label :class="{ error: v$.pageName.$error }">
              {{ $t('INBOX_MGMT.ADD.FB.INBOX_NAME') }}
              <input
                v-model="pageName"
                type="text"
                :placeholder="$t('INBOX_MGMT.ADD.FB.PICK_NAME')"
                @input="v$.pageName.$touch"
              />
              <span v-if="v$.pageName.$error" class="message">
                {{ $t('INBOX_MGMT.ADD.FB.ADD_NAME') }}
              </span>
            </label>
          </div>
          <div class="w-full text-right">
            <NextButton :label="$t('INBOX_MGMT.ADD.FB.CREATE_INBOX')" />
          </div>
        </div>
      </form>
    </div>
  </div>
</template>
