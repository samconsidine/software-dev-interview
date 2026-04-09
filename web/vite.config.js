export default {
  server: {
    proxy: {
      '/config': {
        target: 'http://18.130.238.222:8080',
        changeOrigin: true,
      },
    },
  },
};
