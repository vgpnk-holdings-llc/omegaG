# Portable nginx image for the omegaG marketing site.
# docker build -t omegag-website -f website/Dockerfile website
# docker run --rm -p 8080:80 omegag-website

FROM nginx:1.27-alpine
COPY index.html style.css main.js /usr/share/nginx/html/
COPY assets/ /usr/share/nginx/html/assets/
# SPA-style: no routes beyond index; keep default nginx config
EXPOSE 80
