<template>
    <v-navigation-drawer app>
      <v-list>
        <v-list-item-group>
          <v-list-item>
            <v-list-item-title>File Explorer</v-list-item-title>
          </v-list-item>
          <v-list-item>
            <v-list-item-title>Search</v-list-item-title>
          </v-list-item>
          <v-list-item>
            <v-list-item-title>Source Control</v-list-item-title>
          </v-list-item>
        </v-list-item-group>
      </v-list>
    </v-navigation-drawer>
  </template>
  
  <script>
  export default {
    name: "appSidebar",
  };
  </script>
  