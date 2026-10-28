<script setup>
import { computed, ref, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import instagramClient from 'dashboard/api/channel/instagramClient';
import Button from 'dashboard/components-next/button/Button.vue';
import Banner from 'dashboard/components-next/banner/Banner.vue';
import Icon from 'dashboard/components-next/icon/Icon.vue';
import { useAccount } from 'dashboard/composables/useAccount';
import { META_RESTRICTION_STATUS_URL } from 'dashboard/constants/globals';

const { t } = useI18n();
const { isMetaInboxCreationDisabled } = useAccount();

const hasError = ref(false);
const errorStateMessage = ref('');
const errorStateDescription = ref('');
const isRequestingAuthorization = ref(false);
const isInstagramConnectionDisabled = computed(
  () => isMetaInboxCreationDisabled.value
);
const isHttpProtocol = computed(
  () => typeof window !== 'undefined' && window.location.protocol === 'http:'
);

const resetAuth = () => {
  hasError.value = false;
  errorStateMessage.value = '';
  errorStateDescription.value = '';
};

onMounted(() => {
  const urlParams = new URLSearchParams(window.location.search);
  //  TODO: Handle error type
  // const errorType = urlParams.get('error_type');
  const errorCode = urlParams.get('code');
  const errorMessage = urlParams.get('error_message');

  if (errorMessage) {
    hasError.value = true;
    if (errorCode === '400') {
      errorStateMessage.value = errorMessage;
      errorStateDescription.value = t('INBOX_MGMT.ADD.INSTAGRAM.ERROR_AUTH');
    } else {
      errorStateMessage.value = t('INBOX_MGMT.ADD.INSTAGRAM.ERROR_MESSAGE');
      errorStateDescription.value = errorMessage;
    }
  }
  // User need to remove the error params from the url to avoid the error to be shown again after page reload, so that user can try again
  const cleanURL = window.location.pathname;
  window.history.replaceState({}, document.title, cleanURL);
});

const requestAuthorization = async () => {
  if (isInstagramConnectionDisabled.value) return;

  isRequestingAuthorization.value = true;
  const response = await instagramClient.generateAuthorization();
  const {
    data: { url },
  } = response;

  window.location.href = url;
};
</script>

<template>
  <div class="h-full p-6 w-full max-w-full flex-shrink-0 flex-grow-0">
    <div class="flex flex-col items-center justify-start h-full text-center">
      <div v-if="hasError" class="max-w-lg mx-auto text-center space-y-4">
        <h5>{{ errorStateMessage }}</h5>
        <p
          v-if="errorStateDescription"
          v-dompurify-html="errorStateDescription"
        />
        <div class="pt-2">
          <Button
            :label="t('INBOX_MGMT.ADD.INSTAGRAM.TRY_AGAIN')"
            @click="resetAuth"
          />
        </div>
      </div>
      <div
        v-else
        class="flex flex-col items-center justify-center w-full px-8 py-10 text-center rounded-2xl outline outline-1 outline-n-weak"
      >
        <div class="flex flex-col items-center w-full max-w-2xl">
          <h6 class="text-2xl font-medium">
            {{ $t('INBOX_MGMT.ADD.INSTAGRAM.CONNECT_YOUR_INSTAGRAM_PROFILE') }}
          </h6>
          <p class="py-6 text-sm text-n-slate-11">
            {{ $t('INBOX_MGMT.ADD.INSTAGRAM.HELP') }}
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
                  {{ t('INBOX_MGMT.ADD.INSTAGRAM.HTTPS_REQUIRED_TITLE') }}
                </p>
                <p>
                  {{ t('INBOX_MGMT.ADD.INSTAGRAM.HTTPS_REQUIRED_DESC') }}
                </p>
              </div>
            </div>
          </Banner>
          <Button
            class="text-white !rounded-full !px-6 bg-gradient-to-r from-[#833AB4] via-[#FD1D1D] to-[#FCAF45]"
            lg
            icon="i-ri-instagram-line"
            :disabled="
              isRequestingAuthorization || isInstagramConnectionDisabled
            "
            :is-loading="isRequestingAuthorization"
            :label="$t('INBOX_MGMT.ADD.INSTAGRAM.CONTINUE_WITH_INSTAGRAM')"
            @click="requestAuthorization()"
          />
          <Banner
            v-if="isInstagramConnectionDisabled"
            color="amber"
            class="w-full mt-6"
          >
            <div class="flex items-start gap-3 text-start">
              <Icon
                icon="i-lucide-triangle-alert"
                class="flex-shrink-0 size-4 mt-0.5"
              />
              <span>
                {{ $t('INBOX_MGMT.ADD.INSTAGRAM.RESTRICTED_WARNING') }}
                <a
                  :href="META_RESTRICTION_STATUS_URL"
                  class="link underline"
                  rel="noopener noreferrer nofollow"
                  target="_blank"
                >
                  {{ $t('INBOX_MGMT.ADD.INSTAGRAM.STATUS_LINK') }}
                </a>
              </span>
            </div>
          </Banner>
          <div
            class="w-full mt-6 p-4 text-start bg-n-slate-2 border border-n-slate-4 rounded-xl"
          >
            <h4
              class="text-sm font-semibold text-n-slate-12 mb-3 flex items-center gap-2"
            >
              <Icon icon="i-lucide-info" class="size-4 text-n-brand" />
              {{ t('INBOX_MGMT.ADD.INSTAGRAM.REQUIREMENTS_TITLE') }}
            </h4>
            <ul
              class="text-xs text-n-slate-11 space-y-2.5 list-disc list-inside"
            >
              <li>
                <span class="font-medium text-n-slate-12">
                  {{ t('INBOX_MGMT.ADD.INSTAGRAM.REQ_ACCOUNT_TYPE_TITLE') }}
                </span>
                <span>
                  {{ t('INBOX_MGMT.ADD.INSTAGRAM.REQ_ACCOUNT_TYPE_DESC') }}
                </span>
              </li>
              <li>
                <span class="font-medium text-n-slate-12">
                  {{ t('INBOX_MGMT.ADD.INSTAGRAM.REQ_META_APP_TITLE') }}
                </span>
                <span>
                  {{ t('INBOX_MGMT.ADD.INSTAGRAM.REQ_META_APP_DESC') }}
                </span>
                <a
                  href="https://developers.facebook.com/"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="link underline text-n-brand ml-1"
                >
                  {{ t('INBOX_MGMT.ADD.INSTAGRAM.META_DEVELOPERS_LINK') }}
                </a>
              </li>
              <li>
                <span class="font-medium text-n-slate-12">
                  {{ t('INBOX_MGMT.ADD.INSTAGRAM.REQ_REDIRECT_TITLE') }}
                </span>
                <span>
                  {{ t('INBOX_MGMT.ADD.INSTAGRAM.REQ_REDIRECT_DESC') }}
                </span>
              </li>
              <li>
                <span class="font-medium text-n-slate-12">
                  {{ t('INBOX_MGMT.ADD.INSTAGRAM.REQ_ENV_TITLE') }}
                </span>
                <span>
                  {{ t('INBOX_MGMT.ADD.INSTAGRAM.REQ_ENV_DESC') }}
                </span>
              </li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
