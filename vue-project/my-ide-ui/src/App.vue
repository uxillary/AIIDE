
<template>
  <v-app>
    <appSidebar />
    <v-main>
      <v-container>
        <!-- Your IDE components go here -->
      </v-container>
    </v-main>
  </v-app>
</template>

<script>
import Sidebar from "./components/appSidebar.vue";

export default {
  name: "App",
  components: {
    Sidebar,
  },
};
</script>

<style>
#app {
  font-family: Avenir, Helvetica, Arial, sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  text-align: center;
  color: #2c3e50;
  margin-top: 60px;
}
</style>
