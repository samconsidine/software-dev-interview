export default {
  server: {
    proxy: {
      '/config': {
        target: 'https://interview-router.adamohq.com',
        changeOrigin: true,
      },
    },
  },
};
