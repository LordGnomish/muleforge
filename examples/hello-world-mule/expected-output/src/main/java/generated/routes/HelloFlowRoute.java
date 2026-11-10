package generated.routes;

import org.apache.camel.builder.RouteBuilder;
import org.apache.camel.LoggingLevel;
import jakarta.enterprise.context.ApplicationScoped;

@ApplicationScoped
public class HelloFlowRoute extends RouteBuilder {

    @Override
    public void configure() throws Exception {
        from("platform-http:/hello?httpMethodRestrict=GET")
            .routeId("helloFlow")
            .setBody(constant("Hello from a migrated Mule application!"))
            .log(LoggingLevel.INFO, "Served /hello: ${body}")
            ;
    }
}
