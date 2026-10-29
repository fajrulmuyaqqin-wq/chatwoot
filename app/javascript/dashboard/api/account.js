/* global axios */
import ApiClient from './ApiClient';

class AccountAPI extends ApiClient {
  constructor() {
    super('', { accountScoped: true });
  }

  get(accountId) {
    return accountId
      ? axios.get(`${this.apiVersion}/accounts/${accountId}`)
      : super.get();
  }

  createAccount(data) {
    return axios.post(`${this.apiVersion}/accounts`, data);
  }

  async getCacheKeys() {
    const response = await axios.get(
      `/api/v1/accounts/${this.accountIdFromRoute}/cache_keys`
    );
    return response.data.cache_keys;
  }

  getTier(accountId) {
    const id = accountId || this.accountIdFromRoute;
    return axios.get(`${this.apiVersion}/accounts/${id}/tier`);
  }

  setTier(tier, accountId) {
    const id = accountId || this.accountIdFromRoute;
    return axios.post(`${this.apiVersion}/accounts/${id}/tier`, { tier });
  }
}

export default new AccountAPI();
