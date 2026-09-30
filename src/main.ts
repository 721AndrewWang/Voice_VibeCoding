import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import router from "./router";
import { initPlatform } from "./stores/platform";

const app = createApp(App);
app.use(createPinia());
app.use(router);
initPlatform().finally(() => app.mount("#app"));
