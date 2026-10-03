if (process.env.NODE_ENV === 'production') {
  console.log('prod', __DEV__, VERSION_STR);
} else {
  console.log('dev');
}
