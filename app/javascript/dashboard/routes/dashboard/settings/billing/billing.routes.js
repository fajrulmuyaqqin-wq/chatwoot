import { frontendURL } from '../../../../helper/URLHelper';
import PlanSelection from './PlanSelection.vue';

export default {
  routes: [
    {
      path: frontendURL('accounts/:accountId/settings/billing'),
      name: 'billing_settings_index',
      meta: {
        permissions: ['administrator'],
      },
      component: PlanSelection,
    },
  ],
};
